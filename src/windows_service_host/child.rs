use std::{
    ffi::OsStr,
    io,
    mem::size_of,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    os::windows::ffi::OsStrExt,
    path::Path,
    ptr::{null, null_mut},
};

use anyhow::{Context, Result};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Security::{
        DuplicateTokenEx, SecurityImpersonation, SetTokenInformation, TOKEN_ALL_ACCESS,
        TokenPrimary, TokenSessionId,
    },
    System::{
        Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock},
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject,
        },
        RemoteDesktop::{
            WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW, WTSActive, WTSEnumerateSessionsW,
            WTSFreeMemory, WTSGetActiveConsoleSessionId, WTSQueryUserToken,
        },
        Threading::{
            CREATE_NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, GetCurrentProcess,
            OpenProcessToken, PROCESS_INFORMATION, STARTUPINFOW, TerminateProcess,
            WaitForSingleObject,
        },
    },
};

pub(super) const NO_CONSOLE_SESSION: u32 = u32::MAX;

pub(super) struct WorkspaceHostProcess {
    child: ChildProcess,
    pub(super) session_id: u32,
}

// Handles are owned by the wrapper and queried or closed only by its supervisor task.
unsafe impl Send for WorkspaceHostProcess {}

impl WorkspaceHostProcess {
    pub(super) fn spawn(
        executable: &Path,
        session_id: u32,
        address: SocketAddr,
        token: &str,
    ) -> Result<Self> {
        let executable_wide = wide_null(executable.as_os_str());
        let desktop = wide_null(OsStr::new("winsta0\\default"));
        let current_dir = executable.parent().map(|path| wide_null(path.as_os_str()));
        let mut command_wide = host_command(executable, "workspace-host", address, token);
        let mut user_token = null_mut();
        let queried = unsafe { WTSQueryUserToken(session_id, &mut user_token) };
        if queried == 0 {
            return Err(io::Error::last_os_error())
                .context("the active Windows user token could not be queried");
        }

        let mut environment = null_mut();
        let environment_created =
            unsafe { CreateEnvironmentBlock(&mut environment, user_token, 0) };
        if environment_created == 0 {
            let error = io::Error::last_os_error();
            unsafe {
                CloseHandle(user_token);
            }
            return Err(error).context("the active user's environment could not be created");
        }

        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            lpDesktop: desktop.as_ptr().cast_mut(),
            ..STARTUPINFOW::default()
        };
        let mut process_info = PROCESS_INFORMATION::default();
        let created = unsafe {
            CreateProcessAsUserW(
                user_token,
                executable_wide.as_ptr(),
                command_wide.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                environment,
                current_dir
                    .as_ref()
                    .map_or(null(), |directory| directory.as_ptr()),
                &startup,
                &mut process_info,
            )
        };
        let create_error = (created == 0).then(io::Error::last_os_error);
        unsafe {
            DestroyEnvironmentBlock(environment);
            CloseHandle(user_token);
        }
        if let Some(error) = create_error {
            return Err(error).context("the user-owned Latitude workspace host could not start");
        }
        unsafe {
            CloseHandle(process_info.hThread);
        }

        Ok(Self {
            child: ChildProcess::new(process_info.hProcess, "workspace host")?,
            session_id,
        })
    }

    pub(super) fn has_exited(&self) -> bool {
        self.child.has_exited()
    }
}

pub(super) struct SessionHostProcess {
    child: ChildProcess,
    pub(super) session_id: u32,
}

// Handles are owned by the wrapper and queried or closed only by its supervisor task.
unsafe impl Send for SessionHostProcess {}

impl SessionHostProcess {
    pub(super) fn spawn(
        executable: &Path,
        session_id: u32,
        address: SocketAddr,
        token: &str,
    ) -> Result<Self> {
        let executable_wide = wide_null(executable.as_os_str());
        let desktop = wide_null(OsStr::new("winsta0\\default"));
        let current_dir = executable.parent().map(|path| wide_null(path.as_os_str()));
        let mut command_wide = host_command(executable, "session-host", address, token);
        let mut process_token = null_mut();
        let mut session_token = null_mut();

        let opened =
            unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ALL_ACCESS, &mut process_token) };
        if opened == 0 {
            return Err(io::Error::last_os_error())
                .context("Latitude service process token could not be opened");
        }
        let duplicated = unsafe {
            DuplicateTokenEx(
                process_token,
                TOKEN_ALL_ACCESS,
                null(),
                SecurityImpersonation,
                TokenPrimary,
                &mut session_token,
            )
        };
        let duplicate_error = (duplicated == 0).then(io::Error::last_os_error);
        unsafe {
            CloseHandle(process_token);
        }
        if let Some(error) = duplicate_error {
            return Err(error).context("Latitude session-host token could not be duplicated");
        }
        let session_set = unsafe {
            SetTokenInformation(
                session_token,
                TokenSessionId,
                (&session_id as *const u32).cast(),
                size_of::<u32>() as u32,
            )
        };
        let session_error = (session_set == 0).then(io::Error::last_os_error);
        if let Some(error) = session_error {
            unsafe {
                CloseHandle(session_token);
            }
            return Err(error)
                .context("Latitude session-host token could not enter the console session");
        }

        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            lpDesktop: desktop.as_ptr().cast_mut(),
            ..STARTUPINFOW::default()
        };
        let mut process_info = PROCESS_INFORMATION::default();
        let created = unsafe {
            CreateProcessAsUserW(
                session_token,
                executable_wide.as_ptr(),
                command_wide.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_NO_WINDOW,
                null(),
                current_dir
                    .as_ref()
                    .map_or(null(), |directory| directory.as_ptr()),
                &startup,
                &mut process_info,
            )
        };
        let create_error = (created == 0).then(io::Error::last_os_error);
        unsafe {
            CloseHandle(session_token);
        }
        if let Some(error) = create_error {
            return Err(error)
                .context("privileged Latitude session-host process could not be created");
        }
        unsafe {
            CloseHandle(process_info.hThread);
        }

        Ok(Self {
            child: ChildProcess::new(process_info.hProcess, "session host")?,
            session_id,
        })
    }

    pub(super) fn has_exited(&self) -> bool {
        self.child.has_exited()
    }
}

struct ChildProcess {
    job: HANDLE,
    process: HANDLE,
}

impl ChildProcess {
    fn new(process: HANDLE, label: &str) -> Result<Self> {
        Ok(Self {
            job: create_kill_on_close_job(process, label)?,
            process,
        })
    }

    fn has_exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.process, 0) == WAIT_OBJECT_0 }
    }
}

impl Drop for ChildProcess {
    fn drop(&mut self) {
        unsafe {
            if WaitForSingleObject(self.process, 0) == WAIT_TIMEOUT {
                TerminateProcess(self.process, 0);
                WaitForSingleObject(self.process, 2_000);
            }
            CloseHandle(self.process);
            CloseHandle(self.job);
        }
    }
}

fn create_kill_on_close_job(process: HANDLE, label: &str) -> Result<HANDLE> {
    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        let error = io::Error::last_os_error();
        unsafe {
            TerminateProcess(process, 1);
            CloseHandle(process);
        }
        return Err(error).with_context(|| format!("Latitude {label} job could not be created"));
    }
    let mut job_limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    job_limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&job_limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    let assigned = configured != 0 && unsafe { AssignProcessToJobObject(job, process) } != 0;
    if !assigned {
        let error = io::Error::last_os_error();
        unsafe {
            TerminateProcess(process, 1);
            CloseHandle(process);
            CloseHandle(job);
        }
        return Err(error)
            .with_context(|| format!("Latitude {label} could not enter its job object"));
    }
    Ok(job)
}

pub(super) fn reserve_loopback_address() -> Result<SocketAddr> {
    let listener =
        std::net::TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0))?;
    Ok(listener.local_addr()?)
}

pub(super) fn active_interactive_session_id() -> u32 {
    let mut sessions: *mut WTS_SESSION_INFOW = null_mut();
    let mut count = 0;
    let enumerated = unsafe {
        WTSEnumerateSessionsW(WTS_CURRENT_SERVER_HANDLE, 0, 1, &mut sessions, &mut count)
    };
    if enumerated != 0 && !sessions.is_null() {
        let active = unsafe { std::slice::from_raw_parts(sessions, count as usize) }
            .iter()
            .find(|session| session.State == WTSActive && session.SessionId != 0)
            .map(|session| session.SessionId);
        unsafe {
            WTSFreeMemory(sessions.cast());
        }
        if let Some(active) = active {
            return active;
        }
    }

    unsafe { WTSGetActiveConsoleSessionId() }
}

pub(super) fn encode_hex(bytes: impl AsRef<[u8]>) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.as_ref();
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn host_command(executable: &Path, role: &str, address: SocketAddr, token: &str) -> Vec<u16> {
    let command = format!(
        "{} {role} --bind {} --token {}",
        quote_windows_argument(executable.as_os_str()),
        address,
        quote_windows_argument(OsStr::new(token)),
    );
    wide_null(OsStr::new(&command))
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

fn quote_windows_argument(value: &OsStr) -> String {
    let value = value.to_string_lossy();
    if !value
        .chars()
        .any(|character| character.is_ascii_whitespace() || character == '"')
    {
        return value.into_owned();
    }

    let mut output = String::from("\"");
    let mut backslashes = 0;
    for character in value.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                output.push_str(&"\\".repeat(backslashes * 2 + 1));
                output.push('"');
                backslashes = 0;
            }
            _ => {
                output.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                output.push(character);
            }
        }
    }
    output.push_str(&"\\".repeat(backslashes * 2));
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{encode_hex, quote_windows_argument};

    #[test]
    fn quotes_windows_arguments_with_spaces_and_quotes() {
        assert_eq!(
            quote_windows_argument(OsStr::new(r#"C:\Program Files\Latitude\latitude.exe"#)),
            r#""C:\Program Files\Latitude\latitude.exe""#
        );
        assert_eq!(
            quote_windows_argument(OsStr::new(r#"value "with" quotes"#)),
            r#""value \"with\" quotes""#
        );
    }

    #[test]
    fn leaves_simple_windows_arguments_unquoted() {
        assert_eq!(
            quote_windows_argument(OsStr::new("127.0.0.1:7601")),
            "127.0.0.1:7601"
        );
    }

    #[test]
    fn encodes_session_tokens_as_lowercase_hex() {
        assert_eq!(encode_hex([0, 15, 16, 255]), "000f10ff");
    }
}
