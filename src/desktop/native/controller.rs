use std::{
    collections::{HashMap, VecDeque},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::{Mutex, watch};

use super::{
    NativeDesktopCommand, NativeDesktopError, NativeInputState, apply_native_desktop_command,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeControllerLeaseState {
    Pending,
    Granted,
    Waiting,
    Disabled,
}

impl NativeControllerLeaseState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Granted => "granted",
            Self::Waiting => "waiting",
            Self::Disabled => "disabled",
        }
    }
}

struct NativeInputControllerState {
    owner: Option<u64>,
    input: NativeInputState,
    sessions: HashMap<u64, watch::Sender<NativeControllerLeaseState>>,
    waiters: VecDeque<u64>,
}

pub(crate) struct NativeInputController {
    next_session_id: AtomicU64,
    state: Mutex<NativeInputControllerState>,
}

impl NativeInputController {
    fn new() -> Self {
        Self {
            next_session_id: AtomicU64::new(1),
            state: Mutex::new(NativeInputControllerState {
                owner: None,
                input: NativeInputState::default(),
                sessions: HashMap::new(),
                waiters: VecDeque::new(),
            }),
        }
    }

    pub(crate) fn next_session_id(&self) -> u64 {
        self.next_session_id.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) async fn subscribe(
        &self,
        session_id: u64,
        enabled: bool,
    ) -> watch::Receiver<NativeControllerLeaseState> {
        let initial = if enabled {
            NativeControllerLeaseState::Pending
        } else {
            NativeControllerLeaseState::Disabled
        };
        let (state_tx, state_rx) = watch::channel(initial);
        if enabled {
            self.state
                .lock()
                .await
                .sessions
                .insert(session_id, state_tx);
        }
        state_rx
    }

    pub(crate) async fn activate(&self, session_id: u64) {
        let mut state = self.state.lock().await;
        let Some(session) = state.sessions.get(&session_id) else {
            return;
        };
        if *session.borrow() != NativeControllerLeaseState::Pending {
            return;
        }

        if state.owner.is_none() {
            state.owner = Some(session_id);
            if let Some(session) = state.sessions.get(&session_id) {
                session.send_replace(NativeControllerLeaseState::Granted);
            }
        } else {
            state.waiters.push_back(session_id);
            if let Some(session) = state.sessions.get(&session_id) {
                session.send_replace(NativeControllerLeaseState::Waiting);
            }
        }
    }

    pub(crate) async fn apply(
        &self,
        session_id: u64,
        command: NativeDesktopCommand,
    ) -> Result<bool, NativeDesktopError> {
        let mut state = self.state.lock().await;
        if state.owner != Some(session_id) {
            return Ok(false);
        }

        apply_native_desktop_command(command, &mut state.input)?;
        Ok(true)
    }

    pub(crate) async fn unregister(&self, session_id: u64) {
        let mut state = self.state.lock().await;
        state.sessions.remove(&session_id);
        state.waiters.retain(|waiter| *waiter != session_id);
        if state.owner != Some(session_id) {
            return;
        }

        release_native_input(&mut state.input);
        state.owner = None;
        while let Some(next_session_id) = state.waiters.pop_front() {
            let Some(next_session) = state.sessions.get(&next_session_id).cloned() else {
                continue;
            };
            state.owner = Some(next_session_id);
            next_session.send_replace(NativeControllerLeaseState::Granted);
            break;
        }
    }
}

fn release_native_input(state: &mut NativeInputState) {
    if state.buttons != 0 {
        let x = state.x;
        let y = state.y;
        let _ =
            apply_native_desktop_command(NativeDesktopCommand::Pointer { x, y, buttons: 0 }, state);
    }
    if !state.keys.is_empty() {
        let _ = apply_native_desktop_command(NativeDesktopCommand::ReleaseKeys, state);
    }
    *state = NativeInputState::default();
}

pub(crate) fn native_input_controller() -> &'static NativeInputController {
    static CONTROLLER: OnceLock<NativeInputController> = OnceLock::new();
    CONTROLLER.get_or_init(NativeInputController::new)
}

#[cfg(test)]
mod tests {
    use super::{NativeControllerLeaseState, NativeInputController};

    #[tokio::test]
    async fn hands_control_to_the_next_connected_session() {
        let controller = NativeInputController::new();
        let mut first = controller.subscribe(1, true).await;
        let mut second = controller.subscribe(2, true).await;

        controller.activate(1).await;
        first.changed().await.unwrap();
        controller.activate(2).await;
        second.changed().await.unwrap();

        assert_eq!(*first.borrow(), NativeControllerLeaseState::Granted);
        assert_eq!(*second.borrow(), NativeControllerLeaseState::Waiting);

        controller.unregister(1).await;
        second.changed().await.unwrap();

        assert_eq!(*second.borrow(), NativeControllerLeaseState::Granted);
    }
}
