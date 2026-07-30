mod bridge;
mod files;
mod host;
mod process;
mod terminal;

use std::{
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::terminal::TerminalSessionManager;

use files::WorkspaceFiles;
pub(crate) use host::run_workspace_host;

static GLOBAL_WORKSPACE_BRIDGE: OnceLock<WorkspaceBridge> = OnceLock::new();

#[derive(Clone, Debug)]
struct WorkspaceEndpoint {
    address: SocketAddr,
    token: String,
    profile_dir: PathBuf,
}

#[derive(Clone, Default)]
pub(crate) struct WorkspaceBridge {
    endpoint: Arc<RwLock<Option<WorkspaceEndpoint>>>,
}

#[derive(Clone)]
struct WorkspaceHostState {
    token: Arc<str>,
    terminals: Arc<TerminalSessionManager>,
    files: WorkspaceFiles,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct WorkspaceHealth {
    pub(crate) identity: String,
    pub(crate) profile_dir: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WorkspaceExecRequest {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) environment: Vec<(String, String)>,
    pub(crate) timeout_ms: u64,
    pub(crate) max_output_bytes: usize,
    pub(crate) detached: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct WorkspaceProcessOutput {
    pub(crate) status_code: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) duration_ms: u128,
    pub(crate) timed_out: bool,
    pub(crate) truncated: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspaceExecResponse {
    status_code: Option<i32>,
    stdout: String,
    stderr: String,
    duration_ms: u128,
    timed_out: bool,
    truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct WorkspaceTerminalRequest {
    pub(crate) project: Option<String>,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) session: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct WorkspaceFileRequest {
    pub(crate) project_dir: PathBuf,
    #[serde(default)]
    pub(crate) path: String,
    #[serde(default)]
    pub(crate) raw: bool,
    #[serde(default)]
    pub(crate) search: String,
    #[serde(default)]
    pub(crate) search_kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct WorkspaceFileWriteRequest {
    pub(crate) project_dir: PathBuf,
    pub(crate) path: String,
    pub(crate) content: String,
}

pub(crate) fn install_global_workspace_bridge(bridge: WorkspaceBridge) -> Result<()> {
    GLOBAL_WORKSPACE_BRIDGE
        .set(bridge)
        .map_err(|_| anyhow!("the process workspace bridge was already installed"))
}

pub(crate) fn global_workspace_bridge() -> Option<&'static WorkspaceBridge> {
    GLOBAL_WORKSPACE_BRIDGE.get()
}
