use std::time::Duration;

use crate::config::MAX_PAGE_BINARY_CONTENT_BYTES;

pub(super) const DEFAULT_PAGE_TITLE: &str = "Latitude Page";
pub(super) const DIFF_ROUTE_SEGMENT: &str = "_diff";
pub(super) const FILES_ROUTE_SEGMENT: &str = "_files";
pub(super) const DESKTOP_ROUTE_SEGMENT: &str = "_desktop";
pub(super) const TERMINAL_ROUTE_SEGMENT: &str = "_terminal";
pub(super) const TERMINAL_WS_SUFFIX: &str = "ws";
pub(super) const LOGIN_PATH: &str = "/__latitude/login";
pub(super) const T3CODE_EMBED_PATH: &str = "/__latitude/t3code/embed";
pub(super) const T3CODE_EMBED_COOKIE: &str = "latitude_t3code_embed";
pub(super) const PUBLIC_SHARE_BASE_PATH: &str = "/__latitude/share";
pub(super) const PUBLIC_ROOT_TERMINAL_WS_PATH: &str = "/_terminal/ws";
pub(super) const PUBLIC_ROOT_DESKTOP_WS_PATH: &str = "/_desktop/ws";
pub(super) const PUBLIC_TERMINAL_WS_PATH: &str = "/{project}/_terminal/ws";
pub(super) const PUBLIC_API_SESSION_PATH: &str = "/__latitude/api/session";
pub(super) const PUBLIC_API_ROOT_TERMINAL_PATH: &str = "/__latitude/api/terminal";
pub(super) const PUBLIC_API_ROOT_DESKTOP_PATH: &str = "/__latitude/api/desktop";
pub(super) const PUBLIC_API_ROOT_TERMINAL_SESSIONS_PATH: &str = "/__latitude/api/terminal/sessions";
pub(super) const PUBLIC_API_ROOT_TERMINAL_SESSION_PATH: &str =
    "/__latitude/api/terminal/sessions/{session}";
pub(super) const PUBLIC_API_PROJECTS_PATH: &str = "/__latitude/api/projects";
pub(super) const PUBLIC_API_SHARES_PATH: &str = "/__latitude/api/shares";
pub(super) const PUBLIC_API_SHARE_PATH: &str = "/__latitude/api/shares/{token}";
pub(super) const PUBLIC_API_PROJECT_PATH: &str = "/__latitude/api/projects/{project}";
pub(super) const PUBLIC_API_PROJECT_DIFF_PATH: &str = "/__latitude/api/projects/{project}/diff";
pub(super) const PUBLIC_API_PROJECT_GIT_HISTORY_PATH: &str =
    "/__latitude/api/projects/{project}/diff/history";
pub(super) const PUBLIC_API_PROJECT_GIT_COMMIT_PATH: &str =
    "/__latitude/api/projects/{project}/diff/history/{hash}";
pub(super) const PUBLIC_API_PROJECT_FILES_PATH: &str = "/__latitude/api/projects/{project}/files";
pub(super) const MAX_FILE_EDITOR_BYTES: usize = 5 * 1024 * 1024;
pub(super) const PUBLIC_API_PROJECT_TERMINAL_PATH: &str =
    "/__latitude/api/projects/{project}/terminal";
pub(super) const PUBLIC_API_PROJECT_TERMINAL_SESSIONS_PATH: &str =
    "/__latitude/api/projects/{project}/terminal/sessions";
pub(super) const PUBLIC_API_PROJECT_TERMINAL_SESSION_PATH: &str =
    "/__latitude/api/projects/{project}/terminal/sessions/{session}";
pub(super) const LATITUDE_THEME_HEADER: &str = "x-latitude-theme";
pub(super) const LATITUDE_THEME_COOKIE: &str = "latitude_theme";
pub(super) const AUTH_COOKIE_NAME: &str = "latitude_public_session";
pub(super) const AUTH_COOKIE_MAX_AGE_SECONDS: u64 = 60 * 60 * 24;
pub(super) const MAX_LOGIN_PAYLOAD_BYTES: usize = 8 * 1024;
pub(super) const MAX_DIFF_ACTION_PAYLOAD_BYTES: usize = 64 * 1024;
pub(super) const MAX_DESKTOP_ACTION_PAYLOAD_BYTES: usize = 8 * 1024;
pub(super) const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const MAX_TERMINAL_COMMAND_BYTES: usize = 8 * 1024;
pub(super) const MAX_TERMINAL_OUTPUT_BYTES: usize = 128 * 1024;
pub(super) const TERMINAL_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
pub(super) const MAX_PAGE_PAYLOAD_BYTES: usize =
    ((MAX_PAGE_BINARY_CONTENT_BYTES / 3) + 1) * 4 + 8192;
