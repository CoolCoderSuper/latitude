use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{
        Path as AxumPath, Query, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, Method, Request, Response, StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post},
};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Options, Parser, html::push_html};
use serde::{Deserialize, Serialize};
use tokio::{fs, net::TcpListener, process::Command, sync::broadcast, time::timeout};
use tracing::{error, info};

use crate::{
    config::{
        ApplicationConfig, ApplicationTarget, ConfigError, LatitudeConfig, MAX_PAGE_CONTENT_BYTES,
        PageFormat, ProjectConfig,
    },
    state::AppState,
    terminal::{TerminalSession, TerminalSessionSummary},
};

const DEFAULT_PAGE_TITLE: &str = "Latitude Page";
const DIFF_ROUTE_SEGMENT: &str = "_diff";
const TERMINAL_ROUTE_SEGMENT: &str = "_terminal";
const TERMINAL_WS_SUFFIX: &str = "ws";
const LOGIN_PATH: &str = "/__latitude/login";
const PUBLIC_TERMINAL_WS_PATH: &str = "/{project}/_terminal/ws";
const PUBLIC_API_SESSION_PATH: &str = "/__latitude/api/session";
const PUBLIC_API_PROJECTS_PATH: &str = "/__latitude/api/projects";
const PUBLIC_API_PROJECT_PATH: &str = "/__latitude/api/projects/{project}";
const PUBLIC_API_PROJECT_DIFF_PATH: &str = "/__latitude/api/projects/{project}/diff";
const PUBLIC_API_PROJECT_TERMINAL_PATH: &str = "/__latitude/api/projects/{project}/terminal";
const PUBLIC_API_PROJECT_TERMINAL_SESSIONS_PATH: &str =
    "/__latitude/api/projects/{project}/terminal/sessions";
const PUBLIC_API_PROJECT_TERMINAL_SESSION_PATH: &str =
    "/__latitude/api/projects/{project}/terminal/sessions/{session}";
const LATITUDE_THEME_HEADER: &str = "x-latitude-theme";
const AUTH_COOKIE_NAME: &str = "latitude_public_session";
const AUTH_COOKIE_MAX_AGE_SECONDS: u64 = 60 * 60 * 24;
const MAX_LOGIN_PAYLOAD_BYTES: usize = 8 * 1024;
const MAX_DIFF_ACTION_PAYLOAD_BYTES: usize = 64 * 1024;
const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_TERMINAL_COMMAND_BYTES: usize = 8 * 1024;
const MAX_TERMINAL_OUTPUT_BYTES: usize = 128 * 1024;
const TERMINAL_COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PAGE_PAYLOAD_BYTES: usize = MAX_PAGE_CONTENT_BYTES + 4096;
const AUTH_PAGE_STYLE: &str = r#"
:root {
  color-scheme: light dark;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  background: #f7f8fb;
  color: #1c2433;
}

body {
  margin: 0;
  min-height: 100vh;
}

main {
  box-sizing: border-box;
  width: min(100%, 420px);
  margin: 0 auto;
  padding: clamp(40px, 12vh, 96px) 20px;
}

h1,
p {
  margin: 0;
}

h1 {
  font-size: 2rem;
  line-height: 1.15;
}

p {
  margin-top: 8px;
  color: #526173;
}

form {
  display: grid;
  gap: 14px;
  margin-top: 28px;
}

label {
  display: grid;
  gap: 7px;
  color: #334155;
  font-weight: 700;
}

input,
button {
  box-sizing: border-box;
  width: 100%;
  min-height: 44px;
  border-radius: 6px;
  font: inherit;
}

input {
  border: 1px solid #b7c1cf;
  padding: 0 12px;
  color: inherit;
  background: #fff;
}

button {
  border: 1px solid #0f766e;
  color: #fff;
  background: #0f766e;
  font-weight: 700;
  cursor: pointer;
}

button:hover {
  background: #115e59;
}

.error {
  margin-top: 18px;
  border: 1px solid #fecdd3;
  border-radius: 8px;
  padding: 10px 12px;
  color: #991b1b;
  background: #fff1f2;
}

@media (prefers-color-scheme: dark) {
  :root {
    background: #10141d;
    color: #dbe4ef;
  }

  p {
    color: #aeb9c7;
  }

  label {
    color: #dbe4ef;
  }

  input {
    border-color: #2f3a4a;
    background: #151c28;
    color: #dbe4ef;
  }

  .error {
    border-color: #7f1d1d;
    color: #fecdd3;
    background: #3f151b;
  }
}
"#;
const PROJECT_HOME_STYLE: &str = r#"
:root {
  color-scheme: light dark;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  background: #f7f8fb;
  color: #1c2433;
}

body {
  margin: 0;
  min-height: 100vh;
}

main {
  box-sizing: border-box;
  width: min(100%, 760px);
  margin: 0 auto;
  padding: 40px 20px;
}

h1 {
  margin: 0 0 8px;
  font-size: 2rem;
  line-height: 1.15;
}

p {
  margin: 0 0 24px;
  color: #526173;
}

ul {
  display: grid;
  gap: 10px;
  margin: 0;
  padding: 0;
  list-style: none;
}

li {
  border: 1px solid #d7dde8;
  border-radius: 8px;
  background: #fff;
}

a {
  display: block;
  padding: 14px 16px;
  color: inherit;
  text-decoration: none;
}

a:hover {
  background: #eef6f4;
}

strong,
span {
  display: block;
}

span {
  margin-top: 4px;
  color: #64748b;
  font-size: 0.92rem;
}

.empty {
  padding: 14px 16px;
  border: 1px solid #d7dde8;
  border-radius: 8px;
  background: #fff;
}

@media (prefers-color-scheme: dark) {
  :root {
    background: #10141d;
    color: #dbe4ef;
  }

  p,
  span {
    color: #aeb9c7;
  }

  li,
  .empty {
    border-color: #2f3a4a;
    background: #151c28;
  }

  a:hover {
    background: #1d2d2f;
  }
}
"#;
const DIFF_VIEWER_STYLE: &str = r#"
:root {
  color-scheme: light dark;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  background: #f7f8fb;
  color: #1c2433;
}

body {
  margin: 0;
  min-height: 100vh;
}

main {
  box-sizing: border-box;
  width: min(100%, 1180px);
  margin: 0 auto;
  padding: 32px 18px 56px;
}

header {
  display: grid;
  gap: 8px;
  margin-bottom: 24px;
}

h1,
h2,
p {
  margin: 0;
}

h1 {
  font-size: clamp(2rem, 5vw, 3.4rem);
  line-height: 1.08;
}

h2 {
  font-size: 1rem;
}

p {
  color: #526173;
}

a {
  color: #0f766e;
  text-decoration: none;
}

a:hover {
  text-decoration: underline;
}

button,
input {
  font: inherit;
}

button {
  min-height: 40px;
  border: 1px solid #b7c1cf;
  border-radius: 6px;
  padding: 0 14px;
  color: #1c2433;
  background: #fff;
  font-weight: 700;
  cursor: pointer;
}

button:hover {
  border-color: #0f766e;
}

input {
  min-width: min(360px, 100%);
  min-height: 40px;
  box-sizing: border-box;
  border: 1px solid #b7c1cf;
  border-radius: 6px;
  padding: 0 12px;
  color: inherit;
  background: #fff;
}

.project-path {
  overflow-wrap: anywhere;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  font-size: 0.92rem;
}

.action-panel {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: 10px;
  margin: 20px 0;
}

.diff-workspace {
  display: grid;
  gap: 16px;
}

.diff-workspace .action-panel {
  margin: 0;
}

.diff-workspace[aria-busy="true"] button,
.diff-workspace[aria-busy="true"] input {
  opacity: 0.72;
}

.action-status {
  border: 1px solid #a8d0c9;
  border-radius: 8px;
  padding: 10px 14px;
  color: #0f766e;
  background: #ecfdf3;
}

.action-status[hidden] {
  display: none;
}

.file-panel {
  border: 1px solid #d7dde8;
  border-radius: 8px;
  background: #fff;
  overflow: hidden;
}

.file-list {
  display: grid;
  gap: 10px;
  padding: 12px;
}

.file-card {
  border: 1px solid #e0e6ef;
  border-radius: 8px;
  background: #fff;
  overflow: hidden;
}

.file-card[open] {
  border-color: #a8d0c9;
}

.file-summary {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 12px;
  align-items: center;
  padding: 10px 14px;
  cursor: pointer;
}

.file-summary::-webkit-details-marker {
  display: none;
}

.file-summary::before {
  content: ">";
  color: #64748b;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
}

.file-card[open] .file-summary::before {
  content: "v";
}

.status-code {
  min-width: 2.4rem;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  color: #64748b;
}

.file-path {
  overflow-wrap: anywhere;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  font-size: 0.92rem;
}

.file-path span {
  color: #64748b;
}

.file-count {
  color: #64748b;
  font-size: 0.82rem;
}

.file-content {
  border-top: 1px solid #e6ebf2;
}

.file-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 12px 14px;
  border-bottom: 1px solid #e6ebf2;
}

.file-diff {
  border-top: 1px solid #e6ebf2;
}

.file-diff:first-child {
  border-top: 0;
}

.commit-form {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: 10px;
}

.section-heading {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 14px;
  border-bottom: 1px solid #d7dde8;
}

.section-heading code {
  color: #64748b;
  font-size: 0.78rem;
  text-align: right;
  overflow-wrap: anywhere;
}

.empty,
.error {
  padding: 14px;
  color: #64748b;
}

.error {
  color: #991b1b;
  background: #fff1f2;
}

.action-status.error {
  border-color: #fecdd3;
  color: #991b1b;
  background: #fff1f2;
}

pre {
  margin: 0;
  overflow-x: auto;
  padding: 14px 0;
  background: #fbfcfe;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  font-size: 0.86rem;
  line-height: 1.5;
  tab-size: 2;
}

.file-diff-title {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  border-bottom: 1px solid #e6ebf2;
  color: #526173;
  font-size: 0.82rem;
}

.file-diff-title code {
  overflow-wrap: anywhere;
  text-align: right;
}

.line {
  display: block;
  min-width: max-content;
  padding: 0 14px;
  white-space: pre;
}

.line.file {
  color: #334155;
  font-weight: 700;
}

.line.hunk {
  color: #1d4ed8;
}

.line.add {
  color: #166534;
  background: #ecfdf3;
}

.line.remove {
  color: #991b1b;
  background: #fff1f2;
}

.tok-keyword {
  color: #7c3aed;
  font-weight: 700;
}

.tok-string {
  color: #a16207;
}

.tok-comment {
  color: #64748b;
  font-style: italic;
}

.tok-number {
  color: #0f766e;
}

.tok-type {
  color: #0369a1;
}

.tok-property {
  color: #be123c;
}

.tok-punctuation {
  color: #64748b;
}

@media (prefers-color-scheme: dark) {
  :root {
    background: #10141d;
    color: #dbe4ef;
  }

  p,
  .empty,
  .section-heading code {
    color: #aeb9c7;
  }

  .file-panel {
    border-color: #2f3a4a;
    background: #151c28;
  }

  .action-status {
    border-color: #367064;
    color: #bbf7d0;
    background: #123421;
  }

  .action-status.error {
    border-color: #7f1d1d;
    color: #fecdd3;
    background: #3f151b;
  }

  .file-card {
    border-color: #2f3a4a;
    background: #151c28;
  }

  .file-card[open] {
    border-color: #367064;
  }

  .file-content,
  .file-actions,
  .file-diff,
  .file-diff-title {
    border-color: #2f3a4a;
  }

  .status-code,
  .file-path span,
  .file-count,
  .file-diff-title {
    color: #aeb9c7;
  }

  button,
  input {
    border-color: #2f3a4a;
    background: #151c28;
    color: #dbe4ef;
  }

  .section-heading {
    border-bottom-color: #2f3a4a;
  }

  pre {
    background: #111827;
  }

  .line.file {
    color: #dbe4ef;
  }

  .line.hunk {
    color: #93c5fd;
  }

  .line.add {
    color: #bbf7d0;
    background: #123421;
  }

  .line.remove {
    color: #fecdd3;
    background: #3f151b;
  }

  .tok-keyword {
    color: #c4b5fd;
  }

  .tok-string {
    color: #fde68a;
  }

  .tok-comment {
    color: #94a3b8;
  }

  .tok-number {
    color: #67e8f9;
  }

  .tok-type {
    color: #93c5fd;
  }

  .tok-property {
    color: #fda4af;
  }

  .tok-punctuation {
    color: #94a3b8;
  }

  .error {
    color: #fecdd3;
    background: #3f151b;
  }
}

@media (max-width: 720px) {
  main {
    padding: 20px 10px 44px;
  }

  header {
    margin-bottom: 16px;
  }

  h1 {
    font-size: 2rem;
  }

  .action-panel,
  .commit-form {
    display: grid;
    grid-template-columns: 1fr;
  }

  .action-panel form,
  .commit-form input,
  button {
    width: 100%;
  }

  .section-heading,
  .file-diff-title {
    display: grid;
    gap: 6px;
  }

  .section-heading code,
  .file-diff-title code {
    text-align: left;
  }

  .file-list {
    padding: 8px;
  }

  .file-summary {
    grid-template-columns: auto 1fr;
  }

  .file-count {
    grid-column: 2;
  }

  .file-actions {
    display: grid;
    grid-template-columns: 1fr;
  }

  pre {
    font-size: 0.78rem;
  }

  .line {
    padding: 0 10px;
  }
}
"#;
const DIFF_VIEWER_SCRIPT: &str = r#"
const workspace = document.querySelector('[data-diff-workspace]');

if (workspace) {
  const actionUrl = workspace.dataset.actionUrl || window.location.pathname;
  const statusBox = () => workspace.querySelector('[data-action-status]');

  const showStatus = (message, isError) => {
    const box = statusBox();
    if (!box) {
      return;
    }

    box.hidden = false;
    box.textContent = message;
    box.classList.toggle('error', Boolean(isError));
  };

  const hideStatus = () => {
    const box = statusBox();
    if (!box) {
      return;
    }

    box.hidden = true;
    box.textContent = '';
    box.classList.remove('error');
  };

  const openFilePaths = () => new Set(
    Array.from(workspace.querySelectorAll('details.file-card[open][data-file-path]'))
      .map((card) => card.dataset.filePath)
      .filter(Boolean),
  );

  const restoreOpenFiles = (paths) => {
    workspace.querySelectorAll('details.file-card[data-file-path]').forEach((card) => {
      if (paths.has(card.dataset.filePath)) {
        card.open = true;
      }
    });
  };

  const setDisabled = (disabled) => {
    workspace
      .querySelectorAll('button[data-git-action], input[data-commit-message]')
      .forEach((control) => {
        control.disabled = disabled;
      });
  };

  const actionButton = (target) => (
    target instanceof Element ? target.closest('button[data-git-action]') : null
  );

  workspace.addEventListener('keydown', (event) => {
    if (
      event.key !== 'Enter'
      || !(event.target instanceof Element)
      || !event.target.matches('[data-commit-message]')
    ) {
      return;
    }

    event.preventDefault();
    workspace.querySelector('button[data-git-action="commit"]')?.click();
  });

  workspace.addEventListener('click', async (event) => {
    const button = actionButton(event.target);
    if (!button || !workspace.contains(button)) {
      return;
    }

    event.preventDefault();

    const action = button.dataset.gitAction;
    const body = new URLSearchParams({ action });
    if (button.dataset.path) {
      body.set('path', button.dataset.path);
    }

    if (action === 'commit') {
      const input = workspace.querySelector('[data-commit-message]');
      const message = input ? input.value.trim() : '';
      if (!message) {
        showStatus('Commit message required.', true);
        input?.focus();
        return;
      }

      body.set('message', message);
    }

    const openPaths = openFilePaths();
    const scrollY = window.scrollY;

    workspace.setAttribute('aria-busy', 'true');
    setDisabled(true);
    showStatus('Working...', false);

    try {
      const response = await fetch(actionUrl, {
        method: 'PATCH',
        headers: {
          Accept: 'application/json',
          'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8',
        },
        body,
      });
      const payload = await response.json().catch(() => null);
      if (!payload || typeof payload.workspace_html !== 'string') {
        throw new Error(`Action failed (${response.status}).`);
      }

      workspace.innerHTML = payload.workspace_html;
      restoreOpenFiles(openPaths);
      window.scrollTo(0, scrollY);

      if (payload.ok) {
        hideStatus();
      } else {
        showStatus(payload.error || 'Action failed.', true);
      }
    } catch (error) {
      showStatus(error instanceof Error ? error.message : 'Action failed.', true);
    } finally {
      workspace.removeAttribute('aria-busy');
      setDisabled(false);
    }
  });
}
"#;
const TERMINAL_VIEWER_STYLE: &str = r#"
:root {
  color-scheme: light dark;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  background: #f4f7f6;
  color: #1c2433;
}

html,
body {
  margin: 0;
  height: 100%;
}

main {
  box-sizing: border-box;
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 12px;
  height: 100%;
  padding: 12px;
}

header {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
  border: 1px solid #d7dfdc;
  border-radius: 8px;
  padding: 10px 12px;
  background: #fff;
}

p {
  margin: 0;
}

h1 {
  margin: 0;
  color: #18201f;
  font-size: 1rem;
  font-weight: 900;
}

p {
  color: #52615e;
  font-size: 0.86rem;
  font-weight: 700;
}

a {
  color: #0f766e;
  font-size: 0.86rem;
  font-weight: 800;
  text-decoration: none;
  white-space: nowrap;
}

a:hover {
  text-decoration: underline;
}

.project-path {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
  font-family: "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  text-align: right;
}

.terminal-workspace {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  overflow: hidden;
  position: relative;
  min-height: 0;
  border: 1px solid #1b2624;
  border-radius: 8px;
  background: #101514;
}

.terminal-session-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  border-bottom: 1px solid #2e3936;
  padding: 8px;
  background: #171d1b;
}

.terminal-session-list {
  display: flex;
  flex: 1;
  gap: 8px;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: thin;
}

.terminal-session-item {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 4px;
}

.terminal-session-chip,
.terminal-session-close,
.terminal-new-button {
  border: 1px solid #2e3936;
  border-radius: 8px;
  color: #edf4f1;
  background: #101514;
  cursor: pointer;
  font: inherit;
}

.terminal-session-chip {
  display: flex;
  align-items: center;
  gap: 6px;
  max-width: 180px;
  min-height: 36px;
  padding: 0 11px;
  font-size: 0.84rem;
  font-weight: 900;
}

.terminal-session-chip.active {
  border-color: #2aa79c;
  color: #061210;
  background: #2aa79c;
}

.terminal-session-title {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.terminal-session-close,
.terminal-new-button {
  display: inline-grid;
  flex: 0 0 auto;
  place-items: center;
  width: 36px;
  height: 36px;
  padding: 0;
  font-size: 1.2rem;
  font-weight: 900;
}

.terminal-new-button {
  border-color: #2aa79c;
  color: #061210;
  background: #2aa79c;
}

.terminal-session-chip:hover,
.terminal-session-close:hover,
.terminal-new-button:hover {
  filter: brightness(1.08);
}

.terminal-session-chip:disabled,
.terminal-session-close:disabled,
.terminal-new-button:disabled {
  cursor: wait;
  opacity: 0.65;
}

.terminal-stack {
  position: relative;
  min-height: 0;
  background: #101514;
}

.terminal-view {
  position: absolute;
  inset: 0;
  z-index: 0;
  opacity: 0;
  pointer-events: none;
}

.terminal-view.active {
  z-index: 1;
  opacity: 1;
  pointer-events: auto;
}

.terminal-surface {
  box-sizing: border-box;
  width: 100%;
  height: 100%;
  padding: 10px;
}

.terminal-empty {
  padding: 28px;
  color: #aeb9c7;
  font-size: 0.9rem;
  font-weight: 800;
}

.action-status {
  position: absolute;
  top: 12px;
  right: 12px;
  z-index: 2;
  max-width: min(420px, calc(100% - 24px));
  border: 1px solid #367064;
  border-radius: 8px;
  padding: 9px 12px;
  color: #bbf7d0;
  background: rgba(18, 52, 33, 0.96);
  font-size: 0.84rem;
  font-weight: 800;
}

.action-status.error {
  border-color: #7f1d1d;
  color: #fecdd3;
  background: rgba(63, 21, 27, 0.96);
}

.action-status[hidden] {
  display: none;
}

.xterm {
  height: 100%;
}

.xterm .xterm-viewport {
  overflow-y: auto;
}

@media (prefers-color-scheme: dark) {
  :root {
    background: #101514;
    color: #edf4f1;
  }

  header {
    border-color: #2e3936;
    background: #171d1b;
  }

  h1 {
    color: #edf4f1;
  }

  p {
    color: #aeb9c7;
  }
}

@media (max-width: 720px) {
  main {
    gap: 8px;
    padding: 8px;
  }

  header {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 10px;
  }

  .project-path {
    grid-column: 1 / -1;
    text-align: left;
  }

  .terminal-surface {
    padding: 6px;
  }

  .terminal-session-chip {
    max-width: 142px;
  }
}
"#;
const TERMINAL_VIEWER_SCRIPT: &str = r##"
const workspace = document.querySelector('[data-terminal-workspace]');

if (workspace) {
  const sessionList = workspace.querySelector('[data-terminal-sessions]');
  const newButton = workspace.querySelector('[data-terminal-new]');
  const stack = workspace.querySelector('[data-terminal-stack]');
  const status = workspace.querySelector('[data-terminal-status]');
  const empty = workspace.querySelector('[data-terminal-empty]');
  const terminalControllers = new Map();
  const closingSessions = new Set();
  let activeSessionId = null;
  let creatingSession = false;
  let sessions = [];
  let statusTimer = null;

  const showStatus = (message, isError, autoHide) => {
    if (!status) {
      return;
    }

    window.clearTimeout(statusTimer);
    if (!message) {
      status.hidden = true;
      status.textContent = '';
      status.classList.remove('error');
      return;
    }

    status.hidden = false;
    status.textContent = message;
    status.classList.toggle('error', Boolean(isError));
    if (autoHide) {
      statusTimer = window.setTimeout(() => {
        if (!status.classList.contains('error')) {
          showStatus('', false, false);
        }
      }, 1100);
    }
  };

  const sessionsUrl = () =>
    new URL(workspace.dataset.sessionsPath || '/__latitude/api/projects/terminal/sessions', window.location.href);

  const sessionUrl = (sessionId) => {
    const url = sessionsUrl();
    url.pathname = `${url.pathname.replace(/\/$/, '')}/${encodeURIComponent(sessionId)}`;
    return url;
  };

  const buildSocketUrl = (sessionId) => {
    const url = new URL(workspace.dataset.wsPath || `${window.location.pathname.replace(/\/$/, '')}/ws`, window.location.href);
    url.protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    if (workspace.dataset.wsToken) {
      url.searchParams.set('token', workspace.dataset.wsToken);
    }
    if (sessionId) {
      url.searchParams.set('session', sessionId);
    }
    return url;
  };

  const apiFetch = async (url, options = {}) => {
    const headers = new Headers(options.headers || {});
    headers.set('Accept', 'application/json');
    if (workspace.dataset.wsToken) {
      headers.set('Authorization', `Bearer ${workspace.dataset.wsToken}`);
    }

    const response = await fetch(url, {
      ...options,
      credentials: 'same-origin',
      headers,
    });

    if (!response.ok) {
      let message = `${response.status} ${response.statusText}`.trim();
      try {
        const payload = await response.json();
        message = payload.error || payload.message || message;
      } catch (_) {}
      throw new Error(message);
    }

    if (response.status === 204) {
      return null;
    }

    return response.json();
  };

  const sendJson = (socket, payload) => {
    if (socket && socket.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify(payload));
    }
  };

  const socketIsActive = (socket) =>
    socket &&
    (socket.readyState === WebSocket.OPEN ||
      socket.readyState === WebSocket.CONNECTING);

  const terminalOptions = () => ({
    allowProposedApi: false,
    convertEol: true,
    cursorBlink: true,
    cursorStyle: 'block',
    fontFamily: '"SFMono-Regular", Consolas, "Liberation Mono", monospace',
    fontSize: window.matchMedia('(max-width: 720px)').matches ? 12 : 13,
    lineHeight: 1.2,
    scrollback: 5000,
    theme: {
      background: '#101514',
      foreground: '#edf4f1',
      cursor: '#2aa79c',
      selectionBackground: '#2e3936',
      black: '#101514',
      red: '#ff9d87',
      green: '#8fe0ad',
      yellow: '#e1b95a',
      blue: '#9ed2ff',
      magenta: '#c9b6ff',
      cyan: '#73d7e7',
      white: '#edf4f1',
      brightBlack: '#8f9b97',
      brightRed: '#ffd0ca',
      brightGreen: '#c8f2d5',
      brightYellow: '#ffd98b',
      brightBlue: '#c8e4ff',
      brightMagenta: '#e0d6ff',
      brightCyan: '#bdf4fb',
      brightWhite: '#ffffff',
    },
  });

  const createTerminalController = (session) => {
    const existing = terminalControllers.get(session.id);
    if (existing) {
      existing.session = session;
      return existing;
    }

    const view = document.createElement('div');
    view.className = 'terminal-view';
    view.dataset.terminalView = session.id;

    const surface = document.createElement('div');
    surface.className = 'terminal-surface';
    view.append(surface);
    stack.append(view);

    const terminal = new window.Terminal(terminalOptions());
    const fitAddon = new window.FitAddon.FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(surface);

    const controller = {
      session,
      view,
      terminal,
      fitAddon,
      socket: null,
      resizeTimer: null,
      reconnectTimer: null,
      reconnectDelay: 1000,
      hasConnected: false,
      destroyed: false,
      fitAndSend() {
        try {
          this.fitAddon.fit();
        } catch (_) {
          return;
        }
        sendJson(this.socket, {
          type: 'resize',
          cols: this.terminal.cols,
          rows: this.terminal.rows,
        });
      },
      queueResize() {
        window.clearTimeout(this.resizeTimer);
        this.resizeTimer = window.setTimeout(() => this.fitAndSend(), 80);
      },
      clearReconnectTimer() {
        if (this.reconnectTimer) {
          window.clearTimeout(this.reconnectTimer);
          this.reconnectTimer = null;
        }
      },
      scheduleReconnect() {
        if (this.destroyed) {
          return;
        }

        this.clearReconnectTimer();
        const delay = this.reconnectDelay;
        showStatus(`${this.session.title} reconnecting...`, true, false);
        this.reconnectTimer = window.setTimeout(() => {
          this.reconnectTimer = null;
          this.connect();
        }, delay);
        this.reconnectDelay = Math.min(8000, Math.floor(this.reconnectDelay * 1.6));
      },
      connect() {
        if (this.destroyed || socketIsActive(this.socket)) {
          return;
        }

        this.clearReconnectTimer();
        showStatus(`${this.session.title} connecting...`, false, false);
        const nextSocket = new WebSocket(buildSocketUrl(this.session.id).toString());
        this.socket = nextSocket;

        nextSocket.addEventListener('open', () => {
          if (this.socket !== nextSocket) {
            nextSocket.close();
            return;
          }

          this.clearReconnectTimer();
          this.reconnectDelay = 1000;
          if (this.hasConnected) {
            this.terminal.reset();
          }
          this.hasConnected = true;
          showStatus(`${this.session.title} connected.`, false, true);
          this.fitAndSend();
        });

        nextSocket.addEventListener('message', (event) => {
          if (this.socket !== nextSocket) {
            return;
          }

          if (typeof event.data === 'string') {
            this.terminal.write(event.data);
          } else if (event.data instanceof Blob) {
            event.data.text().then((text) => this.terminal.write(text));
          }
        });

        nextSocket.addEventListener('close', () => {
          if (this.socket !== nextSocket) {
            return;
          }

          this.socket = null;
          this.scheduleReconnect();
        });

        nextSocket.addEventListener('error', () => {
          if (this.socket !== nextSocket) {
            return;
          }

          showStatus(`${this.session.title} connection failed.`, true, false);
          try {
            nextSocket.close();
          } catch (_) {
            this.scheduleReconnect();
          }
        });
      },
      reconnect(force) {
        this.clearReconnectTimer();
        this.reconnectDelay = 1000;
        if (force && this.socket && this.socket.readyState !== WebSocket.CLOSED) {
          const staleSocket = this.socket;
          this.socket = null;
          try {
            staleSocket.close();
          } catch (_) {}
        }

        if (socketIsActive(this.socket)) {
          this.fitAndSend();
          return;
        }

        this.connect();
      },
      destroy() {
        this.destroyed = true;
        this.clearReconnectTimer();
        window.clearTimeout(this.resizeTimer);
        if (this.socket) {
          const oldSocket = this.socket;
          this.socket = null;
          try {
            oldSocket.close();
          } catch (_) {}
        }
        this.terminal.dispose();
        this.view.remove();
      },
    };

    terminal.onData((data) => {
      if (!socketIsActive(controller.socket)) {
        controller.reconnect(false);
      }
      sendJson(controller.socket, { type: 'input', data });
    });

    surface.addEventListener('pointerdown', () => terminal.focus(), {
      passive: true,
    });

    terminalControllers.set(session.id, controller);
    controller.connect();
    return controller;
  };

  const renderSessionList = () => {
    sessionList.replaceChildren();
    sessions.forEach((session) => {
      const item = document.createElement('div');
      item.className = 'terminal-session-item';

      const chip = document.createElement('button');
      chip.type = 'button';
      chip.className = 'terminal-session-chip';
      chip.classList.toggle('active', session.id === activeSessionId);
      chip.title = session.title;
      chip.addEventListener('click', () => setActiveSession(session.id));

      const icon = document.createElement('span');
      icon.setAttribute('aria-hidden', 'true');
      icon.textContent = '>_';
      chip.append(icon);

      const title = document.createElement('span');
      title.className = 'terminal-session-title';
      title.textContent = session.title;
      chip.append(title);

      const close = document.createElement('button');
      close.type = 'button';
      close.className = 'terminal-session-close';
      close.disabled = closingSessions.has(session.id);
      close.setAttribute('aria-label', `Close ${session.title}`);
      close.textContent = '\u00d7';
      close.addEventListener('click', (event) => {
        event.stopPropagation();
        closeSession(session.id);
      });

      item.append(chip, close);
      sessionList.append(item);
    });

    newButton.disabled = creatingSession;
    empty.hidden = sessions.length > 0;
  };

  const setActiveSession = (sessionId) => {
    activeSessionId = sessionId;
    terminalControllers.forEach((controller, id) => {
      const active = id === sessionId;
      controller.view.classList.toggle('active', active);
      if (active) {
        controller.queueResize();
        controller.terminal.focus();
      }
    });
    renderSessionList();
  };

  const syncSessions = (nextSessions) => {
    sessions = nextSessions;
    const nextIds = new Set(sessions.map((session) => session.id));
    terminalControllers.forEach((controller, id) => {
      if (!nextIds.has(id)) {
        controller.destroy();
        terminalControllers.delete(id);
      }
    });

    sessions.forEach(createTerminalController);
    if (!activeSessionId || !nextIds.has(activeSessionId)) {
      activeSessionId = sessions[0]?.id || null;
    }

    if (activeSessionId) {
      setActiveSession(activeSessionId);
    } else {
      renderSessionList();
    }
  };

  const loadSessions = async () => {
    showStatus('Loading terminals...', false, false);
    const payload = await apiFetch(sessionsUrl());
    let nextSessions = payload.sessions || [];
    if (nextSessions.length === 0) {
      nextSessions = [await apiFetch(sessionsUrl(), { method: 'POST' })];
    }
    syncSessions(nextSessions);
    showStatus('', false, false);
  };

  const createSession = async () => {
    if (creatingSession) {
      return;
    }

    creatingSession = true;
    renderSessionList();
    showStatus('Creating terminal...', false, false);
    try {
      const created = await apiFetch(sessionsUrl(), { method: 'POST' });
      syncSessions([...sessions, created]);
      setActiveSession(created.id);
      showStatus(`${created.title} ready.`, false, true);
    } catch (error) {
      showStatus(error.message || 'Could not create terminal.', true, false);
    } finally {
      creatingSession = false;
      renderSessionList();
    }
  };

  const closeSession = async (sessionId) => {
    if (closingSessions.has(sessionId)) {
      return;
    }

    closingSessions.add(sessionId);
    renderSessionList();
    try {
      await apiFetch(sessionUrl(sessionId), { method: 'DELETE' });
      const controller = terminalControllers.get(sessionId);
      if (controller) {
        controller.destroy();
        terminalControllers.delete(sessionId);
      }
      const remaining = sessions.filter((session) => session.id !== sessionId);
      activeSessionId =
        activeSessionId === sessionId
          ? remaining[0]?.id || null
          : activeSessionId;
      syncSessions(remaining);
    } catch (error) {
      showStatus(error.message || 'Could not close terminal.', true, false);
    } finally {
      closingSessions.delete(sessionId);
      renderSessionList();
    }
  };

  const reconnectAll = (force) => {
    terminalControllers.forEach((controller) => controller.reconnect(force));
  };

  const resizeAll = () => {
    terminalControllers.forEach((controller) => controller.queueResize());
  };

  const startTerminal = () => {
    if (!sessionList || !newButton || !stack || !empty || !window.Terminal || !window.FitAddon) {
      showStatus('Terminal assets did not load.', true);
      return;
    }

    newButton.addEventListener('click', createSession);
    window.addEventListener('resize', resizeAll);
    window.addEventListener('focus', () => reconnectAll(false));
    window.addEventListener('online', () => reconnectAll(true));
    document.addEventListener('visibilitychange', () => {
      if (document.visibilityState === 'visible') {
        reconnectAll(false);
      }
    });

    loadSessions().catch((error) => {
      showStatus(error.message || 'Could not load terminals.', true, false);
    });
  };

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', startTerminal);
  } else {
    startTerminal();
  }
}
"##;
const PAGE_STYLE: &str = r#"
:root {
  color-scheme: light dark;
  --latitude-page-bg: #f7f8fb;
  --latitude-page-text: #1c2433;
  --latitude-page-heading: #111827;
  --latitude-page-muted: #475569;
  --latitude-page-accent: #0f766e;
  --latitude-page-inline-code-bg: #e8edf3;
  --latitude-page-code-bg: #111827;
  --latitude-page-code-text: #f8fafc;
  --latitude-page-border: #d7dde8;
  font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  background: var(--latitude-page-bg);
  color: var(--latitude-page-text);
}

body {
  margin: 0;
  min-height: 100vh;
  background: var(--latitude-page-bg);
}

.latitude-page {
  box-sizing: border-box;
  width: min(100%, 920px);
  margin: 0 auto;
  padding: clamp(28px, 6vw, 72px) clamp(18px, 4vw, 40px);
  line-height: 1.65;
  font-size: 17px;
}

.latitude-page :first-child {
  margin-top: 0;
}

.latitude-page h1,
.latitude-page h2,
.latitude-page h3 {
  line-height: 1.18;
  margin: 1.8em 0 0.55em;
  color: var(--latitude-page-heading);
}

.latitude-page h1 {
  font-size: clamp(2rem, 5vw, 3.5rem);
}

.latitude-page h2 {
  font-size: 1.65rem;
}

.latitude-page h3 {
  font-size: 1.25rem;
}

.latitude-page p,
.latitude-page ul,
.latitude-page ol,
.latitude-page blockquote,
.latitude-page pre,
.latitude-page table {
  margin: 0 0 1.1em;
}

.latitude-page a {
  color: var(--latitude-page-accent);
}

.latitude-page img,
.latitude-page video,
.latitude-page canvas,
.latitude-page iframe {
  max-width: 100%;
}

.latitude-page blockquote {
  border-left: 4px solid var(--latitude-page-accent);
  padding-left: 1rem;
  color: var(--latitude-page-muted);
}

.latitude-page code {
  border-radius: 6px;
  background: var(--latitude-page-inline-code-bg);
  padding: 0.12em 0.35em;
  font-size: 0.92em;
}

.latitude-page pre {
  overflow-x: auto;
  border-radius: 8px;
  background: var(--latitude-page-code-bg);
  color: var(--latitude-page-code-text);
  padding: 1rem;
}

.latitude-page pre code {
  background: transparent;
  padding: 0;
  color: inherit;
}

.latitude-page table {
  width: 100%;
  border-collapse: collapse;
}

.latitude-page th,
.latitude-page td {
  border-bottom: 1px solid var(--latitude-page-border);
  padding: 0.55rem 0.4rem;
  text-align: left;
}

@media (prefers-color-scheme: dark) {
  :root {
    --latitude-page-bg: #10141d;
    --latitude-page-text: #dbe4ef;
    --latitude-page-heading: #f8fafc;
    --latitude-page-muted: #b8c3d1;
    --latitude-page-inline-code-bg: #1f2937;
    --latitude-page-border: #2f3a4a;
  }
}

:root[data-latitude-theme="light"] {
  color-scheme: light;
  --latitude-page-bg: #f7f8fb;
  --latitude-page-text: #1c2433;
  --latitude-page-heading: #111827;
  --latitude-page-muted: #475569;
  --latitude-page-accent: #0f766e;
  --latitude-page-inline-code-bg: #e8edf3;
  --latitude-page-code-bg: #111827;
  --latitude-page-code-text: #f8fafc;
  --latitude-page-border: #d7dde8;
}

:root[data-latitude-theme="dark"] {
  color-scheme: dark;
  --latitude-page-bg: #10141d;
  --latitude-page-text: #dbe4ef;
  --latitude-page-heading: #f8fafc;
  --latitude-page-muted: #b8c3d1;
  --latitude-page-accent: #2aa79c;
  --latitude-page-inline-code-bg: #1f2937;
  --latitude-page-code-bg: #151b19;
  --latitude-page-code-text: #edf4f1;
  --latitude-page-border: #2f3a4a;
}
"#;

pub async fn run(state: AppState) -> anyhow::Result<()> {
    let config = state.config_snapshot().await;
    let public_bind = config.public_bind.clone();
    let command_bind = config.command_bind.clone();

    let public_listener = TcpListener::bind(&public_bind).await?;
    let command_listener = TcpListener::bind(&command_bind).await?;

    info!(bind = %public_bind, "public proxy listening");
    info!(bind = %command_bind, "command API listening");

    let public_router = public_router(state.clone());
    let command_router = command_router(state);

    tokio::select! {
        result = axum::serve(public_listener, public_router) => {
            result?;
        }
        result = axum::serve(command_listener, command_router) => {
            result?;
        }
    }

    Ok(())
}

fn public_router(state: AppState) -> Router {
    Router::new()
        .route(LOGIN_PATH, get(get_public_login).post(post_public_login))
        .route(
            PUBLIC_API_SESSION_PATH,
            get(public_api_session).post(public_api_login),
        )
        .route(PUBLIC_API_PROJECTS_PATH, get(public_api_list_projects))
        .route(PUBLIC_API_PROJECT_PATH, get(public_api_get_project))
        .route(
            PUBLIC_API_PROJECT_DIFF_PATH,
            get(public_api_get_project_diff).patch(public_api_patch_project_diff),
        )
        .route(
            PUBLIC_API_PROJECT_TERMINAL_PATH,
            get(public_api_get_project_terminal).post(public_api_post_project_terminal),
        )
        .route(
            PUBLIC_API_PROJECT_TERMINAL_SESSIONS_PATH,
            get(public_api_list_terminal_sessions).post(public_api_create_terminal_session),
        )
        .route(
            PUBLIC_API_PROJECT_TERMINAL_SESSION_PATH,
            delete(public_api_delete_terminal_session),
        )
        .route(PUBLIC_TERMINAL_WS_PATH, get(public_terminal_ws))
        .fallback(public_entry)
        .with_state(state)
}

fn command_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/config", get(get_config).put(put_config))
        .route("/projects", get(list_projects).post(create_project))
        .route(
            "/projects/{project}",
            get(get_project).put(replace_project).delete(delete_project),
        )
        .route(
            "/projects/{project}/deployments",
            get(list_project_deployments).post(create_project_deployment),
        )
        .route(
            "/projects/{project}/deployments/{name}",
            get(get_project_deployment)
                .put(replace_project_deployment)
                .delete(delete_project_deployment),
        )
        .route(
            "/projects/{project}/pages/{name}",
            post(upsert_project_page).put(upsert_project_page),
        );

    Router::new()
        .route("/health", get(command_health))
        .nest("/api", api)
        .with_state(state)
}

async fn public_entry(State(state): State<AppState>, req: Request<Body>) -> Response<Body> {
    let original_path = req.uri().path().to_string();
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_auth_challenge(&req, false);
    }

    if original_path == "/" {
        return serve_server_home(req, &config).await;
    }

    let Some(public_path) = split_project_path(&original_path) else {
        return plain_response(
            StatusCode::NOT_FOUND,
            "Latitude is running. Mount a deployment at /{project}/{name} to serve traffic.\n",
        );
    };
    let project_mount = public_path.project_name().to_string();

    let Some(project) = config
        .projects
        .iter()
        .find(|project| project.enabled && project.name == project_mount)
        .cloned()
    else {
        return plain_response(
            StatusCode::NOT_FOUND,
            format!("No enabled project is mounted at /{project_mount}\n"),
        );
    };

    let ProjectPath::Deployment {
        deployment: app_mount,
        remainder,
        ..
    } = public_path
    else {
        return serve_project_home(req, &project).await;
    };

    if app_mount == DIFF_ROUTE_SEGMENT {
        return serve_project_diff(req, &project, remainder.as_str()).await;
    }
    if app_mount == TERMINAL_ROUTE_SEGMENT {
        return serve_project_terminal(req, &project, remainder.as_str()).await;
    }

    let Some(app) = project
        .deployments
        .iter()
        .find(|app| app.enabled && app.name == app_mount)
        .cloned()
    else {
        return plain_response(
            StatusCode::NOT_FOUND,
            format!("No enabled deployment is mounted at /{project_mount}/{app_mount}\n"),
        );
    };

    match &app.target {
        ApplicationTarget::ReverseProxy {
            upstream,
            strip_prefix,
        } => proxy_request(state, req, upstream, *strip_prefix, remainder.as_str()).await,
        ApplicationTarget::Static {
            root,
            index_file,
            spa_fallback,
        } => {
            let root = resolve_project_path(&project.project_dir, root);
            serve_static(req, &root, index_file, *spa_fallback, remainder.as_str()).await
        }
        ApplicationTarget::Page {
            content,
            format,
            title,
        } => serve_page(req, title.as_deref(), *format, content, remainder.as_str()).await,
    }
}

async fn get_public_login(req: Request<Body>) -> Response<Body> {
    let next = clean_next_path(public_login_next_from_query(req.uri().query()));
    public_login_response(StatusCode::OK, &next, false, req.method() == Method::HEAD)
}

async fn post_public_login(State(state): State<AppState>, req: Request<Body>) -> Response<Body> {
    let config = state.config_snapshot().await;
    let query_next = public_login_next_from_query(req.uri().query());
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_LOGIN_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return plain_response(
                StatusCode::BAD_REQUEST,
                format!("login payload could not be read: {error}\n"),
            );
        }
    };
    let form = parse_public_login_form(&body);
    let next = clean_next_path(form.next.or(query_next));

    if public_password_matches(&form.password, &config.public_password) {
        return public_login_success_response(
            &next,
            public_auth_set_cookie(&state, &config.public_password),
        );
    }

    public_login_response(StatusCode::UNAUTHORIZED, &next, true, false)
}

async fn public_api_session(
    State(state): State<AppState>,
    req: Request<Body>,
) -> impl IntoResponse {
    let config = state.config_snapshot().await;
    let authenticated = public_request_is_authenticated(&state, &config, &req);

    Json(PublicSessionResponse {
        authenticated,
        projects_href: authenticated.then(|| PUBLIC_API_PROJECTS_PATH.to_string()),
    })
}

async fn public_api_login(
    State(state): State<AppState>,
    Json(payload): Json<PublicLoginPayload>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    if !public_password_matches(&payload.password, &config.public_password) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "incorrect password",
        ));
    }

    let token = state.public_auth_cookie_value(&config.public_password);
    Ok((
        StatusCode::OK,
        [
            (
                header::SET_COOKIE,
                public_auth_set_cookie(&state, &config.public_password),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        Json(PublicLoginResponse {
            token,
            max_age_seconds: AUTH_COOKIE_MAX_AGE_SECONDS,
            projects_href: PUBLIC_API_PROJECTS_PATH.to_string(),
        }),
    ))
}

async fn public_api_list_projects(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let projects = config
        .projects
        .iter()
        .filter(|project| project.enabled)
        .map(public_project_summary)
        .collect();

    Json(PublicProjectListResponse { projects }).into_response()
}

async fn public_api_get_project(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    Json(public_project_detail(project)).into_response()
}

async fn public_api_get_project_diff(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    Json(public_diff_response(
        collect_project_diff(&project.project_dir).await,
    ))
    .into_response()
}

async fn public_api_patch_project_diff(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_DIFF_ACTION_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("action payload could not be read: {error}"),
            );
        }
    };
    let action = match parse_public_git_action_payload(content_type.as_deref(), &body) {
        Ok(action) => action,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
    };

    let action_result = execute_git_action(&project_config.project_dir, action).await;
    if let Err(error) = &action_result {
        error!(%error, project = %project_config.name, "git action failed");
    }

    Json(PublicGitActionResponse {
        ok: action_result.is_ok(),
        error: action_result.err(),
        diff: public_diff_response(collect_project_diff(&project_config.project_dir).await),
    })
    .into_response()
}

async fn public_api_get_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    Json(terminal_info_response(&project, &project_config.project_dir).await).into_response()
}

async fn public_api_post_project_terminal(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_TERMINAL_COMMAND_BYTES + 1024).await {
        Ok(body) => body,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("terminal payload could not be read: {error}"),
            );
        }
    };
    let command = match parse_terminal_command_payload(content_type.as_deref(), &body) {
        Ok(command) => command,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
    };

    Json(execute_terminal_command(&project_config.project_dir, command).await).into_response()
}

async fn public_api_list_terminal_sessions(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    if !config
        .projects
        .iter()
        .any(|item| item.enabled && item.name == project)
    {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    }

    Json(PublicTerminalSessionListResponse {
        sessions: state.terminal_sessions().list_project(&project).await,
    })
    .into_response()
}

async fn public_api_create_terminal_session(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    match state
        .terminal_sessions()
        .create_session(&project, &project_config.project_dir)
        .await
    {
        Ok(session) => Json(session.summary()).into_response(),
        Err(error) => json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
}

async fn public_api_delete_terminal_session(
    AxumPath((project, session)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_request_is_authenticated(&state, &config, &req) {
        return public_api_auth_challenge();
    }

    if !config
        .projects
        .iter()
        .any(|item| item.enabled && item.name == project)
    {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    }

    if state
        .terminal_sessions()
        .close_project_session(&project, &session)
        .await
    {
        StatusCode::NO_CONTENT.into_response()
    } else {
        json_error(
            StatusCode::NOT_FOUND,
            format!("terminal session '{session}' was not found"),
        )
    }
}

async fn public_terminal_ws(
    AxumPath(project): AxumPath<String>,
    Query(query): Query<TerminalWsQuery>,
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response<Body> {
    let config = state.config_snapshot().await;
    if !public_headers_are_authenticated(&state, &config, &headers, query.token.as_deref()) {
        return public_api_auth_challenge();
    }

    let Some(project_config) = config
        .projects
        .iter()
        .find(|item| item.enabled && item.name == project)
        .cloned()
    else {
        return json_error(
            StatusCode::NOT_FOUND,
            format!("project '{project}' was not found"),
        );
    };

    let terminal_sessions = state.terminal_sessions();
    let session = if let Some(session_id) = query.session.as_deref() {
        match terminal_sessions
            .get_project_session(&project, session_id)
            .await
        {
            Some(session) => session,
            None => {
                return json_error(
                    StatusCode::NOT_FOUND,
                    format!("terminal session '{session_id}' was not found"),
                );
            }
        }
    } else {
        match terminal_sessions
            .create_session(&project, &project_config.project_dir)
            .await
        {
            Ok(session) => session,
            Err(error) => return json_error(StatusCode::INTERNAL_SERVER_ERROR, error),
        }
    };

    ws.on_upgrade(move |socket| terminal_websocket_session(socket, session))
}

async fn proxy_request(
    state: AppState,
    req: Request<Body>,
    upstream: &str,
    strip_prefix: bool,
    remainder: &str,
) -> Response<Body> {
    let (parts, body) = req.into_parts();
    let forward_path = if strip_prefix {
        remainder.to_string()
    } else {
        parts.uri.path().to_string()
    };

    let target_url = match join_upstream_url(upstream, &forward_path, parts.uri.query()) {
        Ok(url) => url,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                format!("upstream URL could not be built: {error}"),
            );
        }
    };

    let body_bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                format!("request body could not be read: {error}"),
            );
        }
    };

    let mut builder = state.client().request(parts.method, target_url);
    for (name, value) in &parts.headers {
        if is_hop_by_hop_header(name.as_str()) || *name == header::HOST {
            continue;
        }
        if *name == header::COOKIE {
            if let Some(filtered_cookie) = filtered_cookie_header(value, AUTH_COOKIE_NAME) {
                builder = builder.header(name, filtered_cookie);
            }
            continue;
        }
        builder = builder.header(name, value);
    }

    match builder
        .timeout(Duration::from_secs(60))
        .body(body_bytes)
        .send()
        .await
    {
        Ok(response) => {
            let status = response.status();
            let mut response_builder = Response::builder().status(status);

            for (name, value) in response.headers() {
                if is_hop_by_hop_header(name.as_str()) {
                    continue;
                }
                response_builder = response_builder.header(name, value);
            }

            match response.bytes().await {
                Ok(bytes) => response_builder
                    .body(Body::from(bytes))
                    .unwrap_or_else(internal_response),
                Err(error) => json_error(
                    StatusCode::BAD_GATEWAY,
                    format!("upstream body could not be read: {error}"),
                ),
            }
        }
        Err(error) => json_error(
            StatusCode::BAD_GATEWAY,
            format!("upstream request failed: {error}"),
        ),
    }
}

async fn serve_static(
    req: Request<Body>,
    root: &Path,
    index_file: &str,
    spa_fallback: bool,
    remainder: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "static deployments support GET and HEAD\n",
        );
    }

    let relative_path = match sanitized_relative_path(remainder) {
        Some(path) => path,
        None => return plain_response(StatusCode::BAD_REQUEST, "invalid static path\n"),
    };

    let mut candidate = root.join(relative_path);
    match fs::metadata(&candidate).await {
        Ok(metadata) if metadata.is_dir() => {
            candidate = candidate.join(index_file);
        }
        Ok(_) => {}
        Err(_) if spa_fallback => {
            candidate = root.join(index_file);
        }
        Err(_) => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
    }

    let metadata = match fs::metadata(&candidate).await {
        Ok(metadata) if metadata.is_file() => metadata,
        _ if spa_fallback => match fs::metadata(root.join(index_file)).await {
            Ok(metadata) if metadata.is_file() => {
                candidate = root.join(index_file);
                metadata
            }
            _ => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
        },
        _ => return plain_response(StatusCode::NOT_FOUND, "file not found\n"),
    };

    let content_type = mime_guess::from_path(&candidate)
        .first_or_octet_stream()
        .to_string();

    if req.method() == Method::HEAD {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, metadata.len())
            .body(Body::empty())
            .unwrap_or_else(internal_response);
    }

    match fs::read(&candidate).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CONTENT_LENGTH, bytes.len())
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response),
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("file could not be read: {error}"),
        ),
    }
}

async fn serve_page(
    req: Request<Body>,
    title: Option<&str>,
    format: PageFormat,
    content: &str,
    remainder: &str,
) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "page deployments support GET and HEAD\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "page deployments only serve one document\n",
        );
    }

    let html = render_page_content(
        title,
        format,
        content,
        page_theme_from_headers(req.headers()),
    );
    let bytes = html.into_bytes();

    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if req.method() == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

async fn serve_project_home(req: Request<Body>, project: &ProjectConfig) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "project homes support GET and HEAD\n",
        );
    }

    let html = render_project_home(project);
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if req.method() == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

async fn serve_project_diff(
    req: Request<Body>,
    project: &ProjectConfig,
    remainder: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::PATCH {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "diff viewers support GET, HEAD, and PATCH\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "diff viewers only serve one document\n",
        );
    }

    if method == Method::PATCH {
        let action_result = handle_git_action_request(req, &project.project_dir).await;
        if let Err(error) = &action_result {
            error!(%error, project = %project.name, "git action failed");
        }

        let report = collect_project_diff(&project.project_dir).await;
        return (
            StatusCode::OK,
            Json(GitActionResponse {
                ok: action_result.is_ok(),
                error: action_result.err(),
                workspace_html: render_diff_workspace_fragment(&report),
            }),
        )
            .into_response();
    }

    let report = collect_project_diff(&project.project_dir).await;
    let html = render_project_diff(project, &report);
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if method == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

async fn serve_project_terminal(
    req: Request<Body>,
    project: &ProjectConfig,
    remainder: &str,
) -> Response<Body> {
    let method = req.method().clone();
    if method != Method::GET && method != Method::HEAD && method != Method::POST {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "terminal viewers support GET, HEAD, and POST\n",
        );
    }

    if remainder != "/" {
        return plain_response(
            StatusCode::NOT_FOUND,
            "terminal viewers only serve one document\n",
        );
    }

    if method == Method::POST {
        let content_type = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let (_parts, body) = req.into_parts();
        let body = match to_bytes(body, MAX_TERMINAL_COMMAND_BYTES + 1024).await {
            Ok(body) => body,
            Err(error) => {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    format!("terminal payload could not be read: {error}"),
                );
            }
        };
        let command = match parse_terminal_command_payload(content_type.as_deref(), &body) {
            Ok(command) => command,
            Err(error) => return json_error(StatusCode::BAD_REQUEST, error),
        };

        return Json(execute_terminal_command(&project.project_dir, command).await).into_response();
    }

    let websocket_token = request_bearer_token(&req);
    let info = terminal_info_response(&project.name, &project.project_dir).await;
    let html = render_project_terminal(project, &info, websocket_token.as_deref());
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if method == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

async fn serve_server_home(req: Request<Body>, config: &LatitudeConfig) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD {
        return plain_response(
            StatusCode::METHOD_NOT_ALLOWED,
            "server home supports GET and HEAD\n",
        );
    }

    let html = render_server_home(config);
    let bytes = html.into_bytes();
    let builder = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, bytes.len());

    if req.method() == Method::HEAD {
        builder
            .body(Body::empty())
            .unwrap_or_else(internal_response)
    } else {
        builder
            .body(Body::from(bytes))
            .unwrap_or_else(internal_response)
    }
}

async fn command_health(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config_snapshot().await;
    let deployment_count = config
        .projects
        .iter()
        .map(|project| project.deployments.len())
        .sum();
    Json(HealthResponse {
        status: "ok",
        public_bind: config.public_bind,
        command_bind: config.command_bind,
        project_count: config.projects.len(),
        deployment_count,
    })
}

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.config_snapshot().await)
}

async fn put_config(
    State(state): State<AppState>,
    Json(config): Json<LatitudeConfig>,
) -> Result<impl IntoResponse, ApiError> {
    state.replace_config(config.clone()).await?;
    Ok(Json(config))
}

async fn list_projects(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config_snapshot().await;
    Json(config.projects)
}

async fn create_project(
    State(state): State<AppState>,
    Json(project): Json<ProjectConfig>,
) -> Result<impl IntoResponse, ApiError> {
    project.validate()?;
    let created = project.clone();

    state
        .update_config(|config| -> Result<(), ConfigError> {
            if config.projects.iter().any(|item| item.name == project.name) {
                return Err(ConfigError::Invalid(format!(
                    "project '{}' already exists",
                    project.name
                )));
            }
            config.projects.push(project);
            Ok(())
        })
        .await??;

    Ok((StatusCode::CREATED, Json(created)))
}

async fn get_project(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    config
        .projects
        .into_iter()
        .find(|item| item.name == project)
        .map(Json)
        .ok_or_else(|| ApiError::not_found(format!("project '{project}' was not found")))
}

async fn replace_project(
    AxumPath(name): AxumPath<String>,
    State(state): State<AppState>,
    Json(mut project): Json<ProjectConfig>,
) -> Result<impl IntoResponse, ApiError> {
    if project.name != name {
        project.name = name.clone();
    }
    project.validate()?;
    let replacement = project.clone();

    state
        .update_config(|config| -> Result<(), ConfigError> {
            if let Some(existing) = config
                .projects
                .iter_mut()
                .find(|existing| existing.name == name)
            {
                *existing = project;
                Ok(())
            } else {
                config.projects.push(project);
                Ok(())
            }
        })
        .await??;

    Ok(Json(replacement))
}

async fn delete_project(
    AxumPath(name): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let removed = state
        .update_config(|config| {
            let before = config.projects.len();
            config.projects.retain(|project| project.name != name);
            before != config.projects.len()
        })
        .await?;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found("project was not found"))
    }
}

async fn list_project_deployments(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    config
        .projects
        .into_iter()
        .find(|item| item.name == project)
        .map(|project| Json(project.deployments))
        .ok_or_else(|| ApiError::not_found(format!("project '{project}' was not found")))
}

async fn create_project_deployment(
    AxumPath(project): AxumPath<String>,
    State(state): State<AppState>,
    Json(app): Json<ApplicationConfig>,
) -> Result<impl IntoResponse, ApiError> {
    app.validate()?;
    let created = app.clone();

    state
        .update_config_fallible(|config| -> Result<(), ApiError> {
            let project_config = find_project_mut(config, &project)?;
            if project_config
                .deployments
                .iter()
                .any(|item| item.name == app.name)
            {
                return Err(ApiError::new(
                    StatusCode::BAD_REQUEST,
                    format!(
                        "deployment '{}' already exists in project '{}'",
                        app.name, project
                    ),
                ));
            }
            project_config.deployments.push(app);
            Ok(())
        })
        .await??;

    Ok((StatusCode::CREATED, Json(created)))
}

async fn get_project_deployment(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let config = state.config_snapshot().await;
    let project_config = config
        .projects
        .into_iter()
        .find(|item| item.name == project)
        .ok_or_else(|| ApiError::not_found(format!("project '{project}' was not found")))?;

    project_config
        .deployments
        .into_iter()
        .find(|app| app.name == name)
        .map(Json)
        .ok_or_else(|| {
            ApiError::not_found(format!(
                "deployment '{name}' was not found in project '{project}'"
            ))
        })
}

async fn replace_project_deployment(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    Json(mut app): Json<ApplicationConfig>,
) -> Result<impl IntoResponse, ApiError> {
    if app.name != name {
        app.name = name.clone();
    }
    app.validate()?;
    let replacement = app.clone();

    state
        .update_config_fallible(|config| -> Result<(), ApiError> {
            let project_config = find_project_mut(config, &project)?;
            if let Some(existing) = project_config
                .deployments
                .iter_mut()
                .find(|existing| existing.name == name)
            {
                *existing = app;
            } else {
                project_config.deployments.push(app);
            }
            Ok(())
        })
        .await??;

    Ok(Json(replacement))
}

async fn upsert_project_page(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<impl IntoResponse, ApiError> {
    let content_type = req
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let (_parts, body) = req.into_parts();
    let body = to_bytes(body, MAX_PAGE_PAYLOAD_BYTES)
        .await
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("page payload could not be read: {error}"),
            )
        })?;

    let page = parse_page_payload(content_type.as_deref(), &body)?;
    let app = ApplicationConfig {
        name: name.clone(),
        enabled: true,
        target: ApplicationTarget::Page {
            content: page.content,
            format: page.format,
            title: page.title,
        },
    };
    app.validate()?;
    let replacement = app.clone();

    state
        .update_config_fallible(|config| -> Result<(), ApiError> {
            let project_config = find_project_mut(config, &project)?;
            if let Some(existing) = project_config
                .deployments
                .iter_mut()
                .find(|existing| existing.name == name)
            {
                *existing = app;
            } else {
                project_config.deployments.push(app);
            }
            Ok(())
        })
        .await??;

    Ok(Json(replacement))
}

async fn delete_project_deployment(
    AxumPath((project, name)): AxumPath<(String, String)>,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let removed = state
        .update_config_fallible(|config| -> Result<bool, ApiError> {
            let project_config = find_project_mut(config, &project)?;
            let before = project_config.deployments.len();
            project_config.deployments.retain(|app| app.name != name);
            Ok(before != project_config.deployments.len())
        })
        .await??;

    if removed {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found(format!(
            "deployment '{name}' was not found in project '{project}'"
        )))
    }
}

async fn handle_git_action_request(req: Request<Body>, project_dir: &Path) -> Result<(), String> {
    let (_parts, body) = req.into_parts();
    let body = match to_bytes(body, MAX_DIFF_ACTION_PAYLOAD_BYTES).await {
        Ok(body) => body,
        Err(error) => return Err(format!("action payload could not be read: {error}")),
    };

    match parse_git_action_form(&body) {
        Ok(action) => execute_git_action(project_dir, action).await,
        Err(error) => Err(error),
    }
}

async fn execute_git_action(project_dir: &Path, action: GitAction) -> Result<(), String> {
    let repo_dir = match git_worktree_root(project_dir).await {
        Ok(path) => path,
        Err(error) => return Err(error),
    };

    match action {
        GitAction::StageAll => {
            run_git_action_text(&repo_dir, "Stage all", &["add", "--all"], &[0]).await
        }
        GitAction::StageFile { path } => {
            run_git_action_text_owned(
                &repo_dir,
                "Stage file",
                &["add".to_string(), "--".to_string(), path],
                &[0],
            )
            .await
        }
        GitAction::UnstageAll => unstage_all(&repo_dir).await,
        GitAction::UnstageFile { path } => unstage_file(&repo_dir, path).await,
        GitAction::Commit { message } => {
            run_git_action_text_owned(
                &repo_dir,
                "Commit staged",
                &["commit".to_string(), "-m".to_string(), message],
                &[0],
            )
            .await
        }
        GitAction::Push => run_git_action_text(&repo_dir, "Push", &["push"], &[0]).await,
    }
}

async fn unstage_file(repo_dir: &Path, path: String) -> Result<(), String> {
    let has_head = run_git_command(repo_dir, &["rev-parse", "--verify", "HEAD"], &[0])
        .await
        .is_ok();

    if has_head {
        run_git_action_text_owned(
            repo_dir,
            "Unstage file",
            &[
                "reset".to_string(),
                "-q".to_string(),
                "HEAD".to_string(),
                "--".to_string(),
                path,
            ],
            &[0],
        )
        .await
    } else {
        run_git_action_text_owned(
            repo_dir,
            "Unstage file",
            &[
                "rm".to_string(),
                "--cached".to_string(),
                "-r".to_string(),
                "--ignore-unmatch".to_string(),
                "--".to_string(),
                path,
            ],
            &[0],
        )
        .await
    }
}

async fn unstage_all(repo_dir: &Path) -> Result<(), String> {
    let has_head = run_git_command(repo_dir, &["rev-parse", "--verify", "HEAD"], &[0])
        .await
        .is_ok();

    if has_head {
        run_git_action_text(repo_dir, "Unstage all", &["reset", "-q", "HEAD"], &[0]).await
    } else {
        run_git_action_text(
            repo_dir,
            "Unstage all",
            &["rm", "--cached", "-r", "--ignore-unmatch", "."],
            &[0],
        )
        .await
    }
}

async fn git_worktree_root(project_dir: &Path) -> Result<PathBuf, String> {
    let output = run_git_command(project_dir, &["rev-parse", "--show-toplevel"], &[0]).await?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if path.is_empty() {
        return Err("git rev-parse --show-toplevel returned an empty path".to_string());
    }

    Ok(PathBuf::from(path))
}

async fn run_git_action_text(
    repo_dir: &Path,
    _title: &str,
    args: &[&str],
    success_codes: &[i32],
) -> Result<(), String> {
    run_git_command(repo_dir, args, success_codes)
        .await
        .map(|_| ())
}

async fn run_git_action_text_owned(
    repo_dir: &Path,
    _title: &str,
    args: &[String],
    success_codes: &[i32],
) -> Result<(), String> {
    run_git_command_owned(repo_dir, args, success_codes)
        .await
        .map(|_| ())
}

async fn collect_project_diff(project_dir: &Path) -> GitDiffReport {
    let fallback_dir = fs::canonicalize(project_dir)
        .await
        .unwrap_or_else(|_| project_dir.to_path_buf());
    let repo_dir = git_worktree_root(project_dir)
        .await
        .unwrap_or_else(|_| fallback_dir.clone());
    let status = collect_git_text(
        &repo_dir,
        &["status", "--short", "--branch", "--untracked-files=all"],
        &[0],
    )
    .await;

    if status.output.is_err() {
        return GitDiffReport {
            repo_dir,
            file_changes: Vec::new(),
        };
    }

    let mut file_changes = collect_git_file_changes(&repo_dir)
        .await
        .unwrap_or_default();
    let unstaged_diff =
        collect_git_text(&repo_dir, &["diff", "--no-ext-diff", "--color=never"], &[0]).await;
    let staged_diff = collect_git_text(
        &repo_dir,
        &["diff", "--cached", "--no-ext-diff", "--color=never"],
        &[0],
    )
    .await;
    let untracked_diff = collect_untracked_diff(&repo_dir).await;
    attach_file_diffs(
        &mut file_changes,
        "Unstaged",
        &unstaged_diff,
        section_output(&unstaged_diff),
    );
    attach_file_diffs(
        &mut file_changes,
        "Staged",
        &staged_diff,
        section_output(&staged_diff),
    );
    attach_file_diffs(
        &mut file_changes,
        "Untracked",
        &untracked_diff,
        section_output(&untracked_diff),
    );

    GitDiffReport {
        repo_dir,
        file_changes,
    }
}

async fn collect_git_file_changes(repo_dir: &Path) -> Result<Vec<GitFileChange>, String> {
    let output = run_git_command(
        repo_dir,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        &[0],
    )
    .await?;

    Ok(parse_porcelain_status(&output.stdout))
}

fn attach_file_diffs(
    changes: &mut [GitFileChange],
    label: &str,
    section: &GitSection,
    content: Option<&str>,
) {
    let Some(content) = content else {
        return;
    };

    for diff in parse_diff_file_sections(label, &section.command, content) {
        let Some(change) = changes.iter_mut().find(|change| {
            change.path == diff.path || change.original_path.as_ref() == Some(&diff.path)
        }) else {
            continue;
        };

        change.diffs.push(diff);
    }
}

fn section_output(section: &GitSection) -> Option<&str> {
    section.output.as_ref().ok().map(String::as_str)
}

async fn collect_git_text(project_dir: &Path, args: &[&str], success_codes: &[i32]) -> GitSection {
    let command = git_command_label(args);
    let output = run_git_command(project_dir, args, success_codes)
        .await
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string());

    GitSection { command, output }
}

async fn collect_untracked_diff(project_dir: &Path) -> GitSection {
    let command = git_command_label(&[
        "diff",
        "--no-index",
        "--color=never",
        "--",
        "/dev/null",
        "<untracked-file>",
    ]);
    let files = match run_git_command(
        project_dir,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        &[0],
    )
    .await
    {
        Ok(output) => parse_nul_separated_paths(&output.stdout),
        Err(error) => {
            return GitSection {
                command,
                output: Err(error),
            };
        }
    };

    if files.is_empty() {
        return GitSection {
            command,
            output: Ok(String::new()),
        };
    }

    let mut combined = String::new();
    for file in files {
        let output = run_git_command(
            project_dir,
            &[
                "diff",
                "--no-index",
                "--color=never",
                "--",
                "/dev/null",
                file.as_str(),
            ],
            &[0, 1],
        )
        .await;

        match output {
            Ok(output) => {
                combined.push_str(&String::from_utf8_lossy(&output.stdout));
                if !output.stderr.is_empty() {
                    if !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                    combined.push_str(&String::from_utf8_lossy(&output.stderr));
                }
                if !combined.ends_with('\n') {
                    combined.push('\n');
                }
            }
            Err(error) => {
                if !combined.ends_with('\n') {
                    combined.push('\n');
                }
                combined.push_str("diff --git a/");
                combined.push_str(&file);
                combined.push_str(" b/");
                combined.push_str(&file);
                combined.push('\n');
                combined.push_str(&error);
                combined.push('\n');
            }
        }
    }

    GitSection {
        command,
        output: Ok(combined),
    }
}

async fn run_git_command(
    project_dir: &Path,
    args: &[&str],
    success_codes: &[i32],
) -> Result<GitCommandOutput, String> {
    let owned_args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    run_git_command_owned(project_dir, &owned_args, success_codes).await
}

async fn run_git_command_owned(
    project_dir: &Path,
    args: &[String],
    success_codes: &[i32],
) -> Result<GitCommandOutput, String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(project_dir);

    let output = match timeout(GIT_COMMAND_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(format!(
                "Could not run {}: {error}",
                git_command_label_owned(args)
            ));
        }
        Err(_) => {
            return Err(format!(
                "{} timed out after {} seconds",
                git_command_label_owned(args),
                GIT_COMMAND_TIMEOUT.as_secs()
            ));
        }
    };

    if output
        .status
        .code()
        .is_some_and(|code| success_codes.contains(&code))
    {
        return Ok(GitCommandOutput {
            stdout: output.stdout,
            stderr: output.stderr,
        });
    }

    let status = output
        .status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "terminated".to_string());
    let mut message = format!(
        "{} exited with status {status}",
        git_command_label_owned(args)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stderr.trim().is_empty() {
        message.push_str("\n\n");
        message.push_str(stderr.trim());
    } else if !stdout.trim().is_empty() {
        message.push_str("\n\n");
        message.push_str(stdout.trim());
    }

    Err(message)
}

fn parse_git_action_form(body: &[u8]) -> Result<GitAction, String> {
    let mut action = None;
    let mut message = None;
    let mut path = None;

    for (key, value) in url::form_urlencoded::parse(body) {
        match key.as_ref() {
            "action" => action = Some(value.into_owned()),
            "message" => message = Some(value.into_owned()),
            "path" => path = Some(value.into_owned()),
            _ => {}
        }
    }

    match action.as_deref().map(str::trim) {
        Some("stage_all") => Ok(GitAction::StageAll),
        Some("stage_file") => {
            let path = clean_git_form_path(path)?;
            Ok(GitAction::StageFile { path })
        }
        Some("unstage_all") => Ok(GitAction::UnstageAll),
        Some("unstage_file") => {
            let path = clean_git_form_path(path)?;
            Ok(GitAction::UnstageFile { path })
        }
        Some("commit") => {
            let message = message.unwrap_or_default().trim().to_string();
            if message.is_empty() {
                Err("commit message is required".to_string())
            } else {
                Ok(GitAction::Commit { message })
            }
        }
        Some("push") => Ok(GitAction::Push),
        Some(action) if !action.is_empty() => Err(format!("unknown git action '{action}'")),
        _ => Err("git action is required".to_string()),
    }
}

fn parse_public_git_action_payload(
    content_type: Option<&str>,
    body: &[u8],
) -> Result<GitAction, String> {
    if content_type_media_type(content_type)
        .as_deref()
        .is_some_and(is_json_media_type)
    {
        let payload: PublicGitActionPayload = serde_json::from_slice(body)
            .map_err(|error| format!("git action JSON payload is invalid: {error}"))?;
        return payload.into_git_action();
    }

    parse_git_action_form(body)
}

fn parse_terminal_command_payload(
    content_type: Option<&str>,
    body: &[u8],
) -> Result<String, String> {
    if content_type_media_type(content_type)
        .as_deref()
        .is_some_and(is_json_media_type)
    {
        let payload: TerminalCommandPayload = serde_json::from_slice(body)
            .map_err(|error| format!("terminal JSON payload is invalid: {error}"))?;
        return clean_terminal_command(payload.command);
    }

    let mut command = None;
    for (key, value) in url::form_urlencoded::parse(body) {
        if key == "command" {
            command = Some(value.into_owned());
        }
    }

    clean_terminal_command(command.unwrap_or_default())
}

fn clean_terminal_command(command: String) -> Result<String, String> {
    let command = command.trim().to_string();
    if command.is_empty() {
        return Err("terminal command is required".to_string());
    }
    if command.len() > MAX_TERMINAL_COMMAND_BYTES {
        return Err(format!(
            "terminal command must be at most {MAX_TERMINAL_COMMAND_BYTES} bytes"
        ));
    }

    Ok(command)
}

async fn terminal_info_response(project: &str, project_dir: &Path) -> PublicTerminalInfoResponse {
    let cwd = terminal_cwd(project_dir).await;
    PublicTerminalInfoResponse {
        cwd: display_path(&cwd),
        shell: terminal_shell_name(),
        timeout_seconds: TERMINAL_COMMAND_TIMEOUT.as_secs(),
        max_output_bytes: MAX_TERMINAL_OUTPUT_BYTES,
        sessions_href: format!("{PUBLIC_API_PROJECTS_PATH}/{project}/terminal/sessions"),
    }
}

async fn execute_terminal_command(
    project_dir: &Path,
    command_text: String,
) -> PublicTerminalCommandResponse {
    let cwd = terminal_cwd(project_dir).await;
    let started = Instant::now();
    let mut command = terminal_shell_command(&command_text);
    command
        .current_dir(&cwd)
        .env("NO_COLOR", "1")
        .kill_on_drop(true);

    let result = timeout(TERMINAL_COMMAND_TIMEOUT, command.output()).await;
    let duration_ms = started.elapsed().as_millis();

    match result {
        Ok(Ok(output)) => PublicTerminalCommandResponse {
            command: command_text,
            cwd: display_path(&cwd),
            shell: terminal_shell_name(),
            exit_code: output.status.code(),
            success: output.status.success(),
            stdout: terminal_output_text(&output.stdout),
            stderr: terminal_output_text(&output.stderr),
            duration_ms,
            timed_out: false,
        },
        Ok(Err(error)) => PublicTerminalCommandResponse {
            command: command_text,
            cwd: display_path(&cwd),
            shell: terminal_shell_name(),
            exit_code: None,
            success: false,
            stdout: String::new(),
            stderr: format!("Could not run terminal shell: {error}"),
            duration_ms,
            timed_out: false,
        },
        Err(_) => PublicTerminalCommandResponse {
            command: command_text,
            cwd: display_path(&cwd),
            shell: terminal_shell_name(),
            exit_code: None,
            success: false,
            stdout: String::new(),
            stderr: format!(
                "Command timed out after {} seconds",
                TERMINAL_COMMAND_TIMEOUT.as_secs()
            ),
            duration_ms,
            timed_out: true,
        },
    }
}

async fn terminal_cwd(project_dir: &Path) -> PathBuf {
    fs::canonicalize(project_dir)
        .await
        .unwrap_or_else(|_| project_dir.to_path_buf())
}

fn terminal_shell_name() -> &'static str {
    if cfg!(windows) { "powershell" } else { "sh" }
}

fn terminal_shell_command(command_text: &str) -> Command {
    if cfg!(windows) {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            command_text,
        ]);
        command
    } else {
        let mut command = Command::new("sh");
        command.args(["-lc", command_text]);
        command
    }
}

fn terminal_output_text(bytes: &[u8]) -> String {
    let truncated = bytes.len() > MAX_TERMINAL_OUTPUT_BYTES;
    let visible = if truncated {
        &bytes[..MAX_TERMINAL_OUTPUT_BYTES]
    } else {
        bytes
    };
    let mut output = String::from_utf8_lossy(visible).to_string();
    if truncated {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        output.push_str(&format!(
            "[output truncated after {MAX_TERMINAL_OUTPUT_BYTES} bytes]"
        ));
    }

    output
}

async fn terminal_websocket_session(mut socket: WebSocket, session: Arc<TerminalSession>) {
    session.attach_client();
    for output in session.history() {
        if socket
            .send(Message::Text(
                String::from_utf8_lossy(&output).to_string().into(),
            ))
            .await
            .is_err()
        {
            session.detach_client();
            return;
        }
    }

    let mut output_rx = session.subscribe();

    loop {
        tokio::select! {
            output = output_rx.recv() => {
                match output {
                    Ok(output) => {
                        if socket
                            .send(Message::Text(String::from_utf8_lossy(&output).to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            message = socket.recv() => {
                let Some(message) = message else {
                    break;
                };
                let Ok(message) = message else {
                    break;
                };

                match message {
                    Message::Text(text) => {
                        if let Ok(payload) = serde_json::from_str::<TerminalClientMessage>(&text) {
                            handle_terminal_client_message(payload, &session);
                        }
                    }
                    Message::Binary(bytes) => {
                        session.write_input(&String::from_utf8_lossy(&bytes));
                    }
                    Message::Close(_) => break,
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        }
    }

    session.detach_client();
}

fn handle_terminal_client_message(payload: TerminalClientMessage, session: &TerminalSession) {
    match payload {
        TerminalClientMessage::Input { data } => {
            session.write_input(&data);
        }
        TerminalClientMessage::Resize { cols, rows } => {
            session.resize(cols, rows);
        }
    }
}

fn clean_git_form_path(path: Option<String>) -> Result<String, String> {
    let path = path.unwrap_or_default().trim().replace('\\', "/");
    if path.is_empty() {
        Err("path is required".to_string())
    } else {
        Ok(path)
    }
}

fn parse_porcelain_status(bytes: &[u8]) -> Vec<GitFileChange> {
    let entries = parse_nul_separated_paths(bytes);
    let mut changes = Vec::new();
    let mut index = 0;

    while index < entries.len() {
        let entry = &entries[index];
        index += 1;

        if entry.len() < 4 {
            continue;
        }

        let mut chars = entry.chars();
        let index_status = chars.next().unwrap_or(' ');
        let worktree_status = chars.next().unwrap_or(' ');
        if chars.next() != Some(' ') {
            continue;
        }

        let path = chars.as_str().to_string();
        if path.is_empty() {
            continue;
        }

        let original_path = if matches!(index_status, 'R' | 'C') && index < entries.len() {
            let original = entries[index].clone();
            index += 1;
            Some(original)
        } else {
            None
        };

        changes.push(GitFileChange {
            path,
            original_path,
            index_status,
            worktree_status,
            diffs: Vec::new(),
        });
    }

    changes
}

fn parse_diff_file_sections(label: &str, command: &str, content: &str) -> Vec<GitFileDiff> {
    let mut sections = Vec::new();
    let mut current_path = None::<String>;
    let mut current_content = String::new();

    for line in content.lines() {
        if line.starts_with("diff --git ") {
            if let Some(path) = current_path.take() {
                sections.push(GitFileDiff {
                    label: label.to_string(),
                    command: command.to_string(),
                    path,
                    content: current_content.trim_end().to_string(),
                });
                current_content.clear();
            }

            current_path = diff_git_line_path(line);
        }

        if current_path.is_some() {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }

    if let Some(path) = current_path {
        sections.push(GitFileDiff {
            label: label.to_string(),
            command: command.to_string(),
            path,
            content: current_content.trim_end().to_string(),
        });
    }

    sections
}

fn diff_git_line_path(line: &str) -> Option<String> {
    let rest = line.strip_prefix("diff --git ")?;
    let (_, after_b) = rest.split_once(" b/")?;
    Some(after_b.trim_matches('"').to_string())
}

fn parse_nul_separated_paths(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == b'\0')
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8_lossy(path).replace('\\', "/"))
        .collect()
}

fn git_command_label(args: &[&str]) -> String {
    let mut label = String::from("git");
    for arg in args {
        label.push(' ');
        label.push_str(arg);
    }
    label
}

fn git_command_label_owned(args: &[String]) -> String {
    let mut label = String::from("git");
    for arg in args {
        label.push(' ');
        label.push_str(arg);
    }
    label
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    public_bind: String,
    command_bind: String,
    project_count: usize,
    deployment_count: usize,
}

#[derive(Debug, Deserialize)]
struct PublicLoginPayload {
    password: String,
}

#[derive(Debug, Serialize)]
struct PublicSessionResponse {
    authenticated: bool,
    projects_href: Option<String>,
}

#[derive(Debug, Serialize)]
struct PublicLoginResponse {
    token: String,
    max_age_seconds: u64,
    projects_href: String,
}

#[derive(Debug, Serialize)]
struct PublicProjectListResponse {
    projects: Vec<PublicProjectSummary>,
}

#[derive(Debug, Serialize)]
struct PublicProjectSummary {
    name: String,
    href: String,
    api_href: String,
    summary: String,
    deployment_count: usize,
}

#[derive(Debug, Serialize)]
struct PublicProjectDetail {
    name: String,
    href: String,
    api_href: String,
    summary: String,
    deployment_count: usize,
    diff: PublicProjectDiffLink,
    terminal: PublicProjectTerminalLink,
    deployments: Vec<PublicDeploymentSummary>,
}

#[derive(Debug, Serialize)]
struct PublicProjectDiffLink {
    href: String,
    api_href: String,
    label: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct PublicProjectTerminalLink {
    href: String,
    api_href: String,
    label: &'static str,
    description: &'static str,
}

#[derive(Debug, Serialize)]
struct PublicDeploymentSummary {
    name: String,
    href: String,
    kind: &'static str,
    label: &'static str,
    title: Option<String>,
}

#[derive(Debug, Serialize)]
struct PublicGitDiffResponse {
    repo_dir: String,
    unstaged_count: usize,
    staged_count: usize,
    file_changes: Vec<GitFileChange>,
}

#[derive(Debug, Deserialize)]
struct PublicGitActionPayload {
    action: String,
    path: Option<String>,
    message: Option<String>,
}

impl PublicGitActionPayload {
    fn into_git_action(self) -> Result<GitAction, String> {
        match self.action.trim() {
            "stage_all" => Ok(GitAction::StageAll),
            "stage_file" => Ok(GitAction::StageFile {
                path: clean_git_form_path(self.path)?,
            }),
            "unstage_all" => Ok(GitAction::UnstageAll),
            "unstage_file" => Ok(GitAction::UnstageFile {
                path: clean_git_form_path(self.path)?,
            }),
            "commit" => {
                let message = self.message.unwrap_or_default().trim().to_string();
                if message.is_empty() {
                    Err("commit message is required".to_string())
                } else {
                    Ok(GitAction::Commit { message })
                }
            }
            "push" => Ok(GitAction::Push),
            action if !action.is_empty() => Err(format!("unknown git action '{action}'")),
            _ => Err("git action is required".to_string()),
        }
    }
}

#[derive(Debug, Serialize)]
struct PublicGitActionResponse {
    ok: bool,
    error: Option<String>,
    diff: PublicGitDiffResponse,
}

#[derive(Debug, Serialize)]
struct PublicTerminalInfoResponse {
    cwd: String,
    shell: &'static str,
    timeout_seconds: u64,
    max_output_bytes: usize,
    sessions_href: String,
}

#[derive(Debug, Serialize)]
struct PublicTerminalSessionListResponse {
    sessions: Vec<TerminalSessionSummary>,
}

#[derive(Debug, Deserialize)]
struct TerminalCommandPayload {
    command: String,
}

#[derive(Debug, Deserialize)]
struct TerminalWsQuery {
    token: Option<String>,
    session: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TerminalClientMessage {
    Input { data: String },
    Resize { cols: u16, rows: u16 },
}

#[derive(Debug, Serialize)]
struct PublicTerminalCommandResponse {
    command: String,
    cwd: String,
    shell: &'static str,
    exit_code: Option<i32>,
    success: bool,
    stdout: String,
    stderr: String,
    duration_ms: u128,
    timed_out: bool,
}

#[derive(Debug)]
struct PagePayload {
    content: String,
    format: PageFormat,
    title: Option<String>,
}

#[derive(Debug)]
struct GitDiffReport {
    repo_dir: PathBuf,
    file_changes: Vec<GitFileChange>,
}

#[derive(Debug, Serialize)]
struct GitActionResponse {
    ok: bool,
    error: Option<String>,
    workspace_html: String,
}

#[derive(Debug, PartialEq, Eq)]
enum GitAction {
    StageAll,
    StageFile { path: String },
    UnstageAll,
    UnstageFile { path: String },
    Commit { message: String },
    Push,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct GitFileChange {
    path: String,
    original_path: Option<String>,
    index_status: char,
    worktree_status: char,
    diffs: Vec<GitFileDiff>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct GitFileDiff {
    label: String,
    command: String,
    path: String,
    content: String,
}

impl GitFileChange {
    fn status_label(&self) -> String {
        format!("{}{}", self.index_status, self.worktree_status)
    }

    fn can_stage(&self) -> bool {
        self.index_status == '?' || self.worktree_status != ' '
    }

    fn can_unstage(&self) -> bool {
        self.index_status != ' ' && self.index_status != '?' && self.index_status != '!'
    }
}

#[derive(Clone, Copy)]
enum FileSectionKind {
    Unstaged,
    Staged,
}

impl FileSectionKind {
    fn includes(self, change: &GitFileChange) -> bool {
        match self {
            Self::Unstaged => change.can_stage(),
            Self::Staged => change.can_unstage(),
        }
    }

    fn includes_diff(self, diff: &GitFileDiff) -> bool {
        match self {
            Self::Unstaged => diff.label == "Unstaged" || diff.label == "Untracked",
            Self::Staged => diff.label == "Staged",
        }
    }
}

#[derive(Debug)]
struct GitSection {
    command: String,
    output: Result<String, String>,
}

#[derive(Debug)]
struct GitCommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct JsonPagePayload {
    content: String,
    #[serde(default)]
    format: Option<PageFormat>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Default)]
struct PublicLoginForm {
    password: String,
    next: Option<String>,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response<Body> {
        json_error(self.status, self.message)
    }
}

impl From<ConfigError> for ApiError {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::Invalid(message) => Self::new(StatusCode::BAD_REQUEST, message),
            error => {
                error!(%error, "config operation failed");
                Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ProjectPath {
    Project {
        project: String,
    },
    Deployment {
        project: String,
        deployment: String,
        remainder: String,
    },
}

impl ProjectPath {
    fn project_name(&self) -> &str {
        match self {
            Self::Project { project } | Self::Deployment { project, .. } => project,
        }
    }
}

fn find_project_mut<'a>(
    config: &'a mut LatitudeConfig,
    name: &str,
) -> Result<&'a mut ProjectConfig, ApiError> {
    config
        .projects
        .iter_mut()
        .find(|project| project.name == name)
        .ok_or_else(|| ApiError::not_found(format!("project '{name}' was not found")))
}

fn public_request_is_authenticated(
    state: &AppState,
    config: &LatitudeConfig,
    req: &Request<Body>,
) -> bool {
    public_headers_are_authenticated(state, config, req.headers(), None)
}

fn public_headers_are_authenticated(
    state: &AppState,
    config: &LatitudeConfig,
    headers: &HeaderMap,
    query_token: Option<&str>,
) -> bool {
    header_cookie_value(headers, AUTH_COOKIE_NAME)
        .as_deref()
        .is_some_and(|value| state.verify_public_auth_cookie(&config.public_password, value))
        || header_bearer_token(headers)
            .as_deref()
            .is_some_and(|value| state.verify_public_auth_cookie(&config.public_password, value))
        || query_token
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some_and(|value| state.verify_public_auth_cookie(&config.public_password, value))
}

fn public_api_auth_challenge() -> Response<Body> {
    json_error(StatusCode::UNAUTHORIZED, "authentication required")
}

fn public_auth_challenge(req: &Request<Body>, login_failed: bool) -> Response<Body> {
    if req.method() != Method::GET && req.method() != Method::HEAD || request_wants_json(req) {
        return json_error(StatusCode::UNAUTHORIZED, "authentication required");
    }

    let next = clean_next_path(
        req.uri()
            .path_and_query()
            .map(|path_and_query| path_and_query.as_str().to_string()),
    );
    public_login_response(
        StatusCode::UNAUTHORIZED,
        &next,
        login_failed,
        req.method() == Method::HEAD,
    )
}

fn request_wants_json(req: &Request<Body>) -> bool {
    req.headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("application/json"))
}

fn header_cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    for value in headers.get_all(header::COOKIE) {
        let Ok(raw) = value.to_str() else {
            continue;
        };

        for cookie in raw.split(';') {
            let Some((cookie_name, cookie_value)) = cookie.trim().split_once('=') else {
                continue;
            };
            if cookie_name.trim() == name {
                return Some(cookie_value.trim().to_string());
            }
        }
    }

    None
}

fn request_bearer_token(req: &Request<Body>) -> Option<String> {
    header_bearer_token(req.headers())
}

fn header_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }

    let token = token.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

fn public_auth_set_cookie(state: &AppState, password: &str) -> String {
    let value = state.public_auth_cookie_value(password);
    format!(
        "{AUTH_COOKIE_NAME}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={AUTH_COOKIE_MAX_AGE_SECONDS}"
    )
}

fn public_login_success_response(next: &str, set_cookie: String) -> Response<Body> {
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, next)
        .header(header::SET_COOKIE, set_cookie)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap_or_else(internal_response)
}

fn public_login_response(
    status: StatusCode,
    next: &str,
    login_failed: bool,
    head: bool,
) -> Response<Body> {
    let html = render_public_login(next, login_failed);
    let content_length = html.len();
    let body = if head {
        Body::empty()
    } else {
        Body::from(html)
    };

    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .header(header::CONTENT_LENGTH, content_length)
        .header(header::CACHE_CONTROL, "no-store")
        .body(body)
        .unwrap_or_else(internal_response)
}

fn public_login_next_from_query(query: Option<&str>) -> Option<String> {
    let query = query?;
    url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
        if key == "next" {
            Some(value.into_owned())
        } else {
            None
        }
    })
}

fn parse_public_login_form(body: &[u8]) -> PublicLoginForm {
    let mut form = PublicLoginForm::default();
    for (key, value) in url::form_urlencoded::parse(body) {
        match key.as_ref() {
            "password" => form.password = value.into_owned(),
            "next" => form.next = Some(value.into_owned()),
            _ => {}
        }
    }
    form
}

fn public_password_matches(submitted: &str, expected: &str) -> bool {
    let submitted = submitted.as_bytes();
    let expected = expected.as_bytes();
    let max_len = submitted.len().max(expected.len());
    let mut diff = submitted.len() ^ expected.len();

    for index in 0..max_len {
        let left = submitted.get(index).copied().unwrap_or(0);
        let right = expected.get(index).copied().unwrap_or(0);
        diff |= usize::from(left ^ right);
    }

    diff == 0
}

fn clean_next_path(next: Option<String>) -> String {
    let next = next.unwrap_or_else(|| "/".to_string());
    let next = next.trim();
    if !next.starts_with('/')
        || next.starts_with("//")
        || next.starts_with(LOGIN_PATH)
        || !next.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
    {
        return "/".to_string();
    }

    next.to_string()
}

fn resolve_project_path(project_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project_dir.join(path)
    }
}

fn split_project_path(path: &str) -> Option<ProjectPath> {
    let path = path.trim_start_matches('/');
    if path.is_empty() {
        return None;
    }

    let mut segments = path.splitn(3, '/');
    let project = segments.next()?.to_string();
    if project.is_empty() {
        return None;
    }

    let Some(deployment) = segments.next() else {
        return Some(ProjectPath::Project { project });
    };
    if deployment.is_empty() {
        return if segments.next().is_some() {
            None
        } else {
            Some(ProjectPath::Project { project })
        };
    }

    let remainder = segments
        .next()
        .map(|rest| format!("/{rest}"))
        .unwrap_or_else(|| "/".to_string());

    Some(ProjectPath::Deployment {
        project,
        deployment: deployment.to_string(),
        remainder,
    })
}

fn join_upstream_url(
    upstream: &str,
    forward_path: &str,
    query: Option<&str>,
) -> Result<String, url::ParseError> {
    let path = if forward_path.starts_with('/') {
        forward_path.to_string()
    } else {
        format!("/{forward_path}")
    };

    let mut target = format!("{}{}", upstream.trim_end_matches('/'), path);
    if let Some(query) = query {
        target.push('?');
        target.push_str(query);
    }

    Ok(target.parse::<url::Url>()?.to_string())
}

fn sanitized_relative_path(path: &str) -> Option<PathBuf> {
    let mut output = PathBuf::new();

    for raw_segment in path.trim_start_matches('/').split('/') {
        if raw_segment.is_empty() {
            continue;
        }

        let decoded = percent_decode_str(raw_segment).decode_utf8().ok()?;
        let segment_path = Path::new(decoded.as_ref());
        let mut components = segment_path.components();

        match (components.next(), components.next()) {
            (Some(Component::Normal(value)), None) => output.push(value),
            _ => return None,
        }
    }

    Some(output)
}

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}

fn filtered_cookie_header(value: &HeaderValue, excluded_name: &str) -> Option<String> {
    let raw = value.to_str().ok()?;
    let cookies = raw
        .split(';')
        .filter_map(|cookie| {
            let cookie = cookie.trim();
            let (name, _) = cookie.split_once('=')?;
            if name.trim() == excluded_name {
                None
            } else {
                Some(cookie.to_string())
            }
        })
        .collect::<Vec<_>>();

    if cookies.is_empty() {
        None
    } else {
        Some(cookies.join("; "))
    }
}

fn json_error(status: StatusCode, message: impl Into<String>) -> Response<Body> {
    (
        status,
        Json(ErrorBody {
            error: message.into(),
        }),
    )
        .into_response()
}

fn plain_response(status: StatusCode, body: impl Into<Body>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body.into())
        .unwrap_or_else(internal_response)
}

fn internal_response(_: axum::http::Error) -> Response<Body> {
    Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from("internal server error\n"))
        .expect("static response should be valid")
}

fn parse_page_payload(content_type: Option<&str>, body: &[u8]) -> Result<PagePayload, ApiError> {
    let media_type = content_type_media_type(content_type);

    if media_type.as_deref().is_some_and(is_json_media_type) {
        let payload: JsonPagePayload = serde_json::from_slice(body).map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("page JSON payload is invalid: {error}"),
            )
        })?;
        let title = clean_page_title(payload.title);
        let format = payload
            .format
            .unwrap_or_else(|| infer_page_format(None, &payload.content));

        return Ok(PagePayload {
            content: payload.content,
            format,
            title,
        });
    }

    let content = std::str::from_utf8(body)
        .map_err(|error| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("page payload must be UTF-8 text: {error}"),
            )
        })?
        .to_string();
    let format = infer_page_format(media_type.as_deref(), &content);

    Ok(PagePayload {
        content,
        format,
        title: None,
    })
}

fn content_type_media_type(content_type: Option<&str>) -> Option<String> {
    content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
}

fn is_json_media_type(media_type: &str) -> bool {
    media_type == "application/json" || media_type.ends_with("+json")
}

fn clean_page_title(title: Option<String>) -> Option<String> {
    title
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
}

fn infer_page_format(media_type: Option<&str>, content: &str) -> PageFormat {
    match media_type {
        Some("text/html") | Some("application/xhtml+xml") => PageFormat::Html,
        Some("text/markdown") | Some("text/x-markdown") | Some("text/md") => PageFormat::Markdown,
        _ if looks_like_html(content) => PageFormat::Html,
        _ => PageFormat::Markdown,
    }
}

fn render_page_content(
    title: Option<&str>,
    format: PageFormat,
    content: &str,
    theme: Option<&str>,
) -> String {
    match format {
        PageFormat::Html if is_full_html_document(content) => content.to_string(),
        PageFormat::Html => wrap_page_document(resolved_page_title(title, None), content, theme),
        PageFormat::Markdown => {
            let html = render_markdown(content);
            wrap_page_document(
                resolved_page_title(title, markdown_heading_title(content)),
                &html,
                theme,
            )
        }
    }
}

fn page_theme_from_headers(headers: &HeaderMap) -> Option<&'static str> {
    headers
        .get(LATITUDE_THEME_HEADER)
        .and_then(|value| value.to_str().ok())
        .and_then(clean_page_theme)
}

fn clean_page_theme(theme: &str) -> Option<&'static str> {
    match theme.trim() {
        "light" => Some("light"),
        "dark" => Some("dark"),
        _ => None,
    }
}

fn render_project_home(project: &ProjectConfig) -> String {
    let page_title = format!("{} - Latitude Project", project.name);
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>");
    output.push_str(&escape_html_text(&page_title));
    output.push_str("</title>\n<style>\n");
    output.push_str(PROJECT_HOME_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main>\n<h1>");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("</h1>\n<p>Project tools and deployments</p>\n");

    let enabled_deployments = project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .collect::<Vec<_>>();

    output.push_str("<ul>\n");
    output.push_str("<li><a href=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push('/');
    output.push_str(DIFF_ROUTE_SEGMENT);
    output.push_str(
        "\"><strong>Code changes</strong><span>Review staged and unstaged files</span></a></li>\n",
    );
    output.push_str("<li><a href=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push('/');
    output.push_str(TERMINAL_ROUTE_SEGMENT);
    output.push_str(
        "\"><strong>Terminal</strong><span>Run commands in the project directory</span></a></li>\n",
    );

    for deployment in enabled_deployments {
        output.push_str("<li><a href=\"/");
        output.push_str(&escape_html_text(&project.name));
        output.push('/');
        output.push_str(&escape_html_text(&deployment.name));
        output.push_str("\"><strong>");
        output.push_str(&escape_html_text(&deployment.name));
        output.push_str("</strong><span>");
        output.push_str(deployment_home_label(deployment));
        if let Some(title) = deployment_page_title(deployment) {
            output.push_str(": ");
            output.push_str(&escape_html_text(title));
        }
        output.push_str("</span></a></li>\n");
    }

    if project
        .deployments
        .iter()
        .all(|deployment| !deployment.enabled)
    {
        output.push_str("<li class=\"empty\">No enabled deployments yet.</li>\n");
    }

    output.push_str("</ul>\n");
    output.push_str("</main>\n</body>\n</html>\n");
    output
}

fn render_project_diff(project: &ProjectConfig, report: &GitDiffReport) -> String {
    let page_title = format!("{} code changes - Latitude", project.name);
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>");
    output.push_str(&escape_html_text(&page_title));
    output.push_str("</title>\n<style>\n");
    output.push_str(DIFF_VIEWER_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main>\n<header>\n<a href=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("\">Back to project</a>\n<h1>Code changes</h1>\n<p>");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("</p>\n<p class=\"project-path\">");
    output.push_str(&escape_html_text(&display_path(&report.repo_dir)));
    output.push_str("</p>\n</header>\n");

    render_diff_workspace(&mut output, project, report);

    output.push_str("<script>\n");
    output.push_str(DIFF_VIEWER_SCRIPT);
    output.push_str("\n</script>\n</main>\n</body>\n</html>\n");
    output
}

fn render_project_terminal(
    project: &ProjectConfig,
    info: &PublicTerminalInfoResponse,
    websocket_token: Option<&str>,
) -> String {
    let page_title = format!("{} terminal - Latitude", project.name);
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>");
    output.push_str(&escape_html_text(&page_title));
    output.push_str("</title>\n<style>\n");
    output.push_str(TERMINAL_VIEWER_STYLE);
    output.push_str(
        "\n</style>\n<link rel=\"stylesheet\" href=\"https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.css\" />\n",
    );
    output.push_str("</head>\n<body>\n<main>\n<header>\n<a href=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("\">Back to project</a>\n<h1>Terminal</h1>\n<p>");
    output.push_str(&escape_html_text(&project.name));
    output.push_str("</p>\n<p class=\"project-path\">");
    output.push_str(&escape_html_text(&info.cwd));
    output.push_str("</p>\n</header>\n");
    output.push_str(
        "<section class=\"terminal-workspace\" data-terminal-workspace data-sessions-path=\"",
    );
    output.push_str(&escape_html_text(&info.sessions_href));
    output.push_str("\" data-ws-path=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push('/');
    output.push_str(TERMINAL_ROUTE_SEGMENT);
    output.push('/');
    output.push_str(TERMINAL_WS_SUFFIX);
    output.push('"');
    if let Some(token) = websocket_token {
        output.push_str(" data-ws-token=\"");
        output.push_str(&escape_html_text(token));
        output.push('"');
    }
    output.push_str(">\n");
    output.push_str("<div class=\"action-status\" data-terminal-status hidden></div>\n");
    output.push_str("<div class=\"terminal-session-bar\"><div class=\"terminal-session-list\" data-terminal-sessions></div><button class=\"terminal-new-button\" type=\"button\" data-terminal-new aria-label=\"New terminal\" title=\"New terminal\">+</button></div>\n");
    output.push_str("<div class=\"terminal-stack\" data-terminal-stack><div class=\"terminal-empty\" data-terminal-empty hidden>No terminals. Use + to create one.</div></div>\n");
    output.push_str("</section>\n<script src=\"https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js\"></script>\n");
    output.push_str("<script src=\"https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.min.js\"></script>\n<script>\n");
    output.push_str(TERMINAL_VIEWER_SCRIPT);
    output.push_str("\n</script>\n</main>\n</body>\n</html>\n");
    output
}

fn render_diff_workspace(output: &mut String, project: &ProjectConfig, report: &GitDiffReport) {
    output.push_str("<div class=\"diff-workspace\" data-diff-workspace data-action-url=\"/");
    output.push_str(&escape_html_text(&project.name));
    output.push('/');
    output.push_str(DIFF_ROUTE_SEGMENT);
    output.push_str("\">\n");
    render_diff_workspace_inner(output, report);
    output.push_str("</div>\n");
}

fn render_diff_workspace_fragment(report: &GitDiffReport) -> String {
    let mut output = String::new();
    render_diff_workspace_inner(&mut output, report);
    output
}

fn render_diff_workspace_inner(output: &mut String, report: &GitDiffReport) {
    output.push_str("<div class=\"action-status\" data-action-status hidden></div>\n");
    render_git_action_panel(output);
    render_git_file_panel(output, &report.file_changes);
}

fn render_git_action_panel(output: &mut String) {
    output.push_str("<section class=\"action-panel\">\n");
    render_git_action_button(output, "stage_all", "Stage all");
    render_git_action_button(output, "unstage_all", "Unstage all");
    output.push_str("<div class=\"commit-form\">");
    output.push_str(
        "<input data-commit-message type=\"text\" required placeholder=\"Commit message\" />",
    );
    output.push_str(
        "<button type=\"button\" data-git-action=\"commit\">Commit staged</button></div>\n",
    );
    render_git_action_button(output, "push", "Push");
    output.push_str("</section>\n");
}

fn render_git_action_button(output: &mut String, action: &str, label: &str) {
    output.push_str("<button type=\"button\" data-git-action=\"");
    output.push_str(&escape_html_text(action));
    output.push_str("\">");
    output.push_str(&escape_html_text(label));
    output.push_str("</button>\n");
}

fn render_git_file_panel(output: &mut String, changes: &[GitFileChange]) {
    render_git_file_section(
        output,
        "Unstaged files",
        "No unstaged files.",
        changes,
        FileSectionKind::Unstaged,
    );
    render_git_file_section(
        output,
        "Staged files",
        "No staged files.",
        changes,
        FileSectionKind::Staged,
    );
}

fn render_git_file_section(
    output: &mut String,
    title: &str,
    empty_message: &str,
    changes: &[GitFileChange],
    kind: FileSectionKind,
) {
    let section_changes = changes
        .iter()
        .filter(|change| kind.includes(change))
        .collect::<Vec<_>>();

    output.push_str("<section class=\"file-panel\">\n<div class=\"section-heading\"><h2>");
    output.push_str(&escape_html_text(title));
    output.push_str("</h2><code>");
    output.push_str(&section_changes.len().to_string());
    output.push_str(match section_changes.len() {
        1 => " file",
        _ => " files",
    });
    output.push_str("</code></div>\n");

    if section_changes.is_empty() {
        output.push_str("<div class=\"empty\">");
        output.push_str(&escape_html_text(empty_message));
        output.push_str("</div>\n</section>\n");
        return;
    }

    output.push_str("<div class=\"file-list\">\n");
    for change in section_changes {
        let visible_diffs = change
            .diffs
            .iter()
            .filter(|diff| kind.includes_diff(diff))
            .collect::<Vec<_>>();

        output.push_str("<details class=\"file-card\" data-file-path=\"");
        output.push_str(&escape_html_text(&change.path));
        output.push_str("\"><summary class=\"file-summary\"><div class=\"status-code\">");
        output.push_str(&escape_html_text(&change.status_label()));
        output.push_str("</div><div class=\"file-path\">");
        output.push_str(&escape_html_text(&change.path));
        if let Some(original_path) = &change.original_path {
            output.push_str("<span> from ");
            output.push_str(&escape_html_text(original_path));
            output.push_str("</span>");
        }
        output.push_str("</div><div class=\"file-count\">");
        output.push_str(match visible_diffs.len() {
            0 => "status only",
            1 => "1 diff",
            _ => "diffs",
        });
        if visible_diffs.len() > 1 {
            output.push_str(": ");
            output.push_str(&visible_diffs.len().to_string());
        }
        output.push_str("</div></summary><div class=\"file-content\"><div class=\"file-actions\">");

        match kind {
            FileSectionKind::Unstaged => {
                render_git_file_action_button(output, "stage_file", "Stage", &change.path);
            }
            FileSectionKind::Staged => {
                render_git_file_action_button(output, "unstage_file", "Unstage", &change.path);
            }
        }

        output.push_str("</div>");

        if visible_diffs.is_empty() {
            output.push_str("<div class=\"empty\">No inline diff for this file.</div>");
        } else {
            for diff in visible_diffs {
                output.push_str("<div class=\"file-diff\"><div class=\"file-diff-title\"><strong>");
                output.push_str(&escape_html_text(&diff.label));
                output.push_str("</strong><code>");
                output.push_str(&escape_html_text(&diff.command));
                output.push_str("</code></div>");
                render_diff_code_output(output, &diff.content, &diff.path);
                output.push_str("</div>");
            }
        }

        output.push_str("</div></details>\n");
    }
    output.push_str("</div>\n</section>\n");
}

fn render_git_file_action_button(output: &mut String, action: &str, label: &str, path: &str) {
    output.push_str("<button type=\"button\" data-git-action=\"");
    output.push_str(&escape_html_text(action));
    output.push_str("\" data-path=\"");
    output.push_str(&escape_html_text(path));
    output.push_str("\">");
    output.push_str(&escape_html_text(label));
    output.push_str("</button>\n");
}

fn render_diff_code_output(output: &mut String, content: &str, path: &str) {
    let language = syntax_language_for_path(path);

    output.push_str("<pre>");
    for line in content.lines() {
        output.push_str("<span class=\"line");
        if let Some(class) = diff_line_class(line) {
            output.push(' ');
            output.push_str(class);
        }
        output.push_str("\">");
        render_diff_line_content(output, line, language);
        output.push_str("</span>\n");
    }
    output.push_str("</pre>\n");
}

fn render_diff_line_content(output: &mut String, line: &str, language: SyntaxLanguage) {
    if matches!(diff_line_class(line), Some("file" | "hunk")) {
        output.push_str(&escape_html_text(line));
        return;
    }

    if let Some(first) = line.chars().next() {
        if matches!(first, '+' | '-' | ' ') {
            output.push(first);
            render_syntax_line(output, language, &line[first.len_utf8()..]);
            return;
        }
    }

    render_syntax_line(output, language, line);
}

fn diff_line_class(line: &str) -> Option<&'static str> {
    if line.starts_with("diff --git")
        || line.starts_with("index ")
        || line.starts_with("new file mode ")
        || line.starts_with("deleted file mode ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
    {
        Some("file")
    } else if line.starts_with("@@") {
        Some("hunk")
    } else if line.starts_with('+') {
        Some("add")
    } else if line.starts_with('-') {
        Some("remove")
    } else {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyntaxLanguage {
    Plain,
    Rust,
    JavaScript,
    Css,
    Html,
    Json,
    Config,
}

fn syntax_language_for_path(path: &str) -> SyntaxLanguage {
    let path = Path::new(path);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match file_name.as_str() {
        "cargo.toml" | "cargo.lock" | "package.json" | "tsconfig.json" | "vite.config.js"
        | "vite.config.ts" | "svelte.config.js" | "svelte.config.ts" => {
            return match file_name.rsplit_once('.').map(|(_, ext)| ext) {
                Some("json") => SyntaxLanguage::Json,
                Some("js") | Some("ts") => SyntaxLanguage::JavaScript,
                _ => SyntaxLanguage::Config,
            };
        }
        _ => {}
    }

    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => SyntaxLanguage::Rust,
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "svelte" => SyntaxLanguage::JavaScript,
        "css" | "scss" | "sass" => SyntaxLanguage::Css,
        "html" | "htm" | "xml" | "svg" => SyntaxLanguage::Html,
        "json" => SyntaxLanguage::Json,
        "toml" | "yaml" | "yml" | "env" | "ini" | "conf" | "lock" => SyntaxLanguage::Config,
        _ => SyntaxLanguage::Plain,
    }
}

fn render_syntax_line(output: &mut String, language: SyntaxLanguage, line: &str) {
    if language == SyntaxLanguage::Plain {
        output.push_str(&escape_html_text(line));
        return;
    }

    let mut index = 0;
    while index < line.len() {
        let rest = &line[index..];

        if let Some(length) = comment_token_len(rest, language) {
            render_token_span(output, "tok-comment", &rest[..length]);
            index += length;
            continue;
        }

        let ch = rest.chars().next().expect("rest is not empty");

        if matches!(ch, '"' | '\'' | '`') {
            let length = string_token_len(rest, ch);
            let class = if language == SyntaxLanguage::Json && followed_by_colon(&rest[length..]) {
                "tok-property"
            } else {
                "tok-string"
            };
            render_token_span(output, class, &rest[..length]);
            index += length;
            continue;
        }

        if language == SyntaxLanguage::Css && rest.starts_with('#') {
            let length = css_color_token_len(rest);
            if length > 1 {
                render_token_span(output, "tok-number", &rest[..length]);
                index += length;
                continue;
            }
        }

        if ch.is_ascii_digit() {
            let length = number_token_len(rest);
            render_token_span(output, "tok-number", &rest[..length]);
            index += length;
            continue;
        }

        if is_identifier_start(ch) {
            let length = identifier_token_len(rest);
            let token = &rest[..length];
            if let Some(class) = syntax_identifier_class(language, token, &rest[length..]) {
                render_token_span(output, class, token);
            } else {
                output.push_str(&escape_html_text(token));
            }
            index += length;
            continue;
        }

        if is_punctuation(ch) {
            render_token_span(output, "tok-punctuation", &rest[..ch.len_utf8()]);
            index += ch.len_utf8();
            continue;
        }

        output.push_str(&escape_html_text(&rest[..ch.len_utf8()]));
        index += ch.len_utf8();
    }
}

fn render_token_span(output: &mut String, class: &str, token: &str) {
    output.push_str("<span class=\"");
    output.push_str(class);
    output.push_str("\">");
    output.push_str(&escape_html_text(token));
    output.push_str("</span>");
}

fn comment_token_len(rest: &str, language: SyntaxLanguage) -> Option<usize> {
    match language {
        SyntaxLanguage::Rust | SyntaxLanguage::JavaScript if rest.starts_with("//") => {
            Some(rest.len())
        }
        SyntaxLanguage::Css if rest.starts_with("/*") => {
            Some(rest.find("*/").map(|index| index + 2).unwrap_or(rest.len()))
        }
        SyntaxLanguage::Html if rest.starts_with("<!--") => Some(
            rest.find("-->")
                .map(|index| index + 3)
                .unwrap_or(rest.len()),
        ),
        SyntaxLanguage::Config if rest.starts_with('#') => Some(rest.len()),
        _ => None,
    }
}

fn string_token_len(rest: &str, quote: char) -> usize {
    let mut escaped = false;
    for (index, ch) in rest.char_indices().skip(1) {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return index + ch.len_utf8();
        }
    }
    rest.len()
}

fn css_color_token_len(rest: &str) -> usize {
    let mut length = 1;
    for (index, ch) in rest.char_indices().skip(1) {
        if ch.is_ascii_hexdigit() {
            length = index + ch.len_utf8();
        } else {
            break;
        }
    }
    length
}

fn number_token_len(rest: &str) -> usize {
    let mut length = 0;
    for (index, ch) in rest.char_indices() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_') {
            length = index + ch.len_utf8();
        } else {
            break;
        }
    }
    length
}

fn identifier_token_len(rest: &str) -> usize {
    let mut length = 0;
    for (index, ch) in rest.char_indices() {
        if is_identifier_continue(ch) {
            length = index + ch.len_utf8();
        } else {
            break;
        }
    }
    length
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    ch == '_' || ch == '-' || ch.is_ascii_alphanumeric()
}

fn is_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '{' | '}'
            | '['
            | ']'
            | '('
            | ')'
            | '<'
            | '>'
            | ';'
            | ':'
            | ','
            | '.'
            | '='
            | '+'
            | '-'
            | '*'
            | '/'
            | '!'
            | '?'
            | '|'
            | '&'
            | '%'
    )
}

fn followed_by_colon(rest: &str) -> bool {
    rest.trim_start().starts_with(':')
}

fn syntax_identifier_class(
    language: SyntaxLanguage,
    token: &str,
    following: &str,
) -> Option<&'static str> {
    if is_keyword(language, token) {
        Some("tok-keyword")
    } else if is_type_token(language, token) {
        Some("tok-type")
    } else if language == SyntaxLanguage::Css && followed_by_colon(following) {
        Some("tok-property")
    } else {
        None
    }
}

fn is_keyword(language: SyntaxLanguage, token: &str) -> bool {
    match language {
        SyntaxLanguage::Rust => matches!(
            token,
            "as" | "async"
                | "await"
                | "break"
                | "const"
                | "continue"
                | "crate"
                | "else"
                | "enum"
                | "extern"
                | "false"
                | "fn"
                | "for"
                | "if"
                | "impl"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "Self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "unsafe"
                | "use"
                | "where"
                | "while"
        ),
        SyntaxLanguage::JavaScript => matches!(
            token,
            "as" | "async"
                | "await"
                | "break"
                | "case"
                | "catch"
                | "class"
                | "const"
                | "continue"
                | "default"
                | "else"
                | "export"
                | "extends"
                | "false"
                | "finally"
                | "for"
                | "from"
                | "function"
                | "if"
                | "import"
                | "in"
                | "interface"
                | "let"
                | "new"
                | "null"
                | "return"
                | "switch"
                | "this"
                | "throw"
                | "true"
                | "try"
                | "type"
                | "typeof"
                | "var"
                | "while"
        ),
        SyntaxLanguage::Css => matches!(
            token,
            "and"
                | "from"
                | "important"
                | "keyframes"
                | "media"
                | "not"
                | "only"
                | "supports"
                | "to"
        ),
        SyntaxLanguage::Json => matches!(token, "false" | "null" | "true"),
        SyntaxLanguage::Html => matches!(token, "DOCTYPE"),
        SyntaxLanguage::Config | SyntaxLanguage::Plain => false,
    }
}

fn is_type_token(language: SyntaxLanguage, token: &str) -> bool {
    match language {
        SyntaxLanguage::Rust => {
            matches!(
                token,
                "bool"
                    | "char"
                    | "f32"
                    | "f64"
                    | "i8"
                    | "i16"
                    | "i32"
                    | "i64"
                    | "i128"
                    | "isize"
                    | "str"
                    | "String"
                    | "u8"
                    | "u16"
                    | "u32"
                    | "u64"
                    | "u128"
                    | "usize"
            ) || token
                .chars()
                .next()
                .map(|ch| ch.is_ascii_uppercase())
                .unwrap_or(false)
        }
        SyntaxLanguage::JavaScript | SyntaxLanguage::Html => token
            .chars()
            .next()
            .map(|ch| ch.is_ascii_uppercase())
            .unwrap_or(false),
        _ => false,
    }
}

fn display_path(path: &Path) -> String {
    let path = path.display().to_string();
    path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
}

fn render_public_login(next: &str, login_failed: bool) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>Sign in - Latitude</title>\n<style>\n");
    output.push_str(AUTH_PAGE_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main>\n");
    output.push_str("<h1>Latitude</h1>\n<p>Sign in to continue</p>\n");
    if login_failed {
        output.push_str("<div class=\"error\">Incorrect password.</div>\n");
    }
    output.push_str("<form method=\"post\" action=\"");
    output.push_str(LOGIN_PATH);
    output.push_str("\">\n<input type=\"hidden\" name=\"next\" value=\"");
    output.push_str(&escape_html_text(next));
    output.push_str("\" />\n<label>Password<input name=\"password\" type=\"password\" required autofocus autocomplete=\"current-password\" /></label>\n");
    output.push_str("<button type=\"submit\">Sign in</button>\n</form>\n");
    output.push_str("</main>\n</body>\n</html>\n");
    output
}

fn render_server_home(config: &LatitudeConfig) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>Latitude Projects</title>\n<style>\n");
    output.push_str(PROJECT_HOME_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main>\n<h1>Latitude</h1>\n");
    output.push_str("<p>Available projects</p>\n");

    let enabled_projects = config
        .projects
        .iter()
        .filter(|project| project.enabled)
        .collect::<Vec<_>>();

    if enabled_projects.is_empty() {
        output.push_str("<div class=\"empty\">No enabled projects yet.</div>\n");
    } else {
        output.push_str("<ul>\n");
        for project in enabled_projects {
            output.push_str("<li><a href=\"/");
            output.push_str(&escape_html_text(&project.name));
            output.push_str("\"><strong>");
            output.push_str(&escape_html_text(&project.name));
            output.push_str("</strong><span>");
            output.push_str(&project_summary(project));
            output.push_str("</span></a></li>\n");
        }
        output.push_str("</ul>\n");
    }

    output.push_str("</main>\n</body>\n</html>\n");
    output
}

fn public_project_summary(project: &ProjectConfig) -> PublicProjectSummary {
    let deployment_count = enabled_deployment_count(project);
    PublicProjectSummary {
        name: project.name.clone(),
        href: format!("/{}", project.name),
        api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}", project.name),
        summary: project_summary(project),
        deployment_count,
    }
}

fn public_project_detail(project: &ProjectConfig) -> PublicProjectDetail {
    let deployments = project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .map(public_deployment_summary(project))
        .collect::<Vec<_>>();

    PublicProjectDetail {
        name: project.name.clone(),
        href: format!("/{}", project.name),
        api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}", project.name),
        summary: project_summary(project),
        deployment_count: deployments.len(),
        diff: PublicProjectDiffLink {
            href: format!("/{}/{}", project.name, DIFF_ROUTE_SEGMENT),
            api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}/diff", project.name),
            label: "Code changes",
            description: "Review staged and unstaged files",
        },
        terminal: PublicProjectTerminalLink {
            href: format!("/{}/{}", project.name, TERMINAL_ROUTE_SEGMENT),
            api_href: format!("{PUBLIC_API_PROJECTS_PATH}/{}/terminal", project.name),
            label: "Terminal",
            description: "Run commands in the project directory",
        },
        deployments,
    }
}

fn public_deployment_summary(
    project: &ProjectConfig,
) -> impl Fn(&ApplicationConfig) -> PublicDeploymentSummary + '_ {
    |deployment| PublicDeploymentSummary {
        name: deployment.name.clone(),
        href: format!("/{}/{}", project.name, deployment.name),
        kind: deployment_kind(deployment),
        label: deployment_home_label(deployment),
        title: deployment_page_title(deployment).map(str::to_string),
    }
}

fn public_diff_response(report: GitDiffReport) -> PublicGitDiffResponse {
    let unstaged_count = report
        .file_changes
        .iter()
        .filter(|change| FileSectionKind::Unstaged.includes(change))
        .count();
    let staged_count = report
        .file_changes
        .iter()
        .filter(|change| FileSectionKind::Staged.includes(change))
        .count();

    PublicGitDiffResponse {
        repo_dir: display_path(&report.repo_dir),
        unstaged_count,
        staged_count,
        file_changes: report.file_changes,
    }
}

fn project_summary(project: &ProjectConfig) -> String {
    let enabled_deployment_count = enabled_deployment_count(project);

    match enabled_deployment_count {
        1 => "1 deployment".to_string(),
        count => format!("{count} deployments"),
    }
}

fn enabled_deployment_count(project: &ProjectConfig) -> usize {
    project
        .deployments
        .iter()
        .filter(|deployment| deployment.enabled)
        .count()
}

fn deployment_kind(deployment: &ApplicationConfig) -> &'static str {
    match &deployment.target {
        ApplicationTarget::ReverseProxy { .. } => "reverse_proxy",
        ApplicationTarget::Static { .. } => "static",
        ApplicationTarget::Page { .. } => "page",
    }
}

fn deployment_home_label(deployment: &ApplicationConfig) -> &'static str {
    match &deployment.target {
        ApplicationTarget::ReverseProxy { .. } => "Website",
        ApplicationTarget::Static { .. } => "Static website",
        ApplicationTarget::Page { .. } => "Page",
    }
}

fn deployment_page_title(deployment: &ApplicationConfig) -> Option<&str> {
    match &deployment.target {
        ApplicationTarget::Page { title, .. } => title.as_deref(),
        _ => None,
    }
}

fn render_markdown(content: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(content, options);
    let mut output = String::new();
    push_html(&mut output, parser);
    output
}

fn wrap_page_document(title: &str, body_html: &str, theme: Option<&str>) -> String {
    let mut output = String::new();
    output.push_str("<!doctype html>\n<html lang=\"en\"");
    if let Some(theme) = theme.and_then(clean_page_theme) {
        output.push_str(" data-latitude-theme=\"");
        output.push_str(theme);
        output.push('"');
    }
    output.push_str(">\n<head>\n");
    output.push_str("<meta charset=\"utf-8\" />\n");
    output.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n");
    output.push_str("<title>");
    output.push_str(&escape_html_text(title));
    output.push_str("</title>\n<style>\n");
    output.push_str(PAGE_STYLE);
    output.push_str("\n</style>\n</head>\n<body>\n<main class=\"latitude-page\">\n");
    output.push_str(body_html);
    output.push_str("\n</main>\n</body>\n</html>\n");
    output
}

fn resolved_page_title<'a>(explicit: Option<&'a str>, derived: Option<&'a str>) -> &'a str {
    explicit
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .or_else(|| derived.map(str::trim).filter(|title| !title.is_empty()))
        .unwrap_or(DEFAULT_PAGE_TITLE)
}

fn markdown_heading_title(content: &str) -> Option<&str> {
    for line in content.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("# ") else {
            continue;
        };
        let title = rest.trim().trim_end_matches('#').trim();
        if !title.is_empty() {
            return Some(title);
        }
    }

    None
}

fn is_full_html_document(content: &str) -> bool {
    let trimmed = content.trim_start().to_ascii_lowercase();
    trimmed.starts_with("<!doctype html") || trimmed.starts_with("<html")
}

fn looks_like_html(content: &str) -> bool {
    let trimmed = content.trim_start();
    trimmed.starts_with('<') && trimmed.contains('>')
}

fn escape_html_text(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(ch),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_project_home_and_deployment_paths() {
        assert_eq!(
            split_project_path("/demo/website1/about"),
            Some(ProjectPath::Deployment {
                project: "demo".to_string(),
                deployment: "website1".to_string(),
                remainder: "/about".to_string()
            })
        );
        assert_eq!(
            split_project_path("/demo/website1"),
            Some(ProjectPath::Deployment {
                project: "demo".to_string(),
                deployment: "website1".to_string(),
                remainder: "/".to_string()
            })
        );
        assert_eq!(
            split_project_path("/demo"),
            Some(ProjectPath::Project {
                project: "demo".to_string()
            })
        );
        assert_eq!(
            split_project_path("/demo/"),
            Some(ProjectPath::Project {
                project: "demo".to_string()
            })
        );
        assert_eq!(split_project_path("/demo//website1"), None);
        assert_eq!(split_project_path("/"), None);
    }

    #[test]
    fn resolves_relative_paths_against_project_dir() {
        assert_eq!(
            resolve_project_path(Path::new("projects/demo"), Path::new("dist")),
            PathBuf::from("projects/demo").join("dist")
        );
    }

    #[test]
    fn rejects_path_traversal_for_static_files() {
        assert!(sanitized_relative_path("/assets/app.js").is_some());
        assert!(sanitized_relative_path("/../secret.txt").is_none());
        assert!(sanitized_relative_path("/%2e%2e/secret.txt").is_none());
        assert!(sanitized_relative_path("/nested%2fsecret.txt").is_none());
    }

    #[test]
    fn authenticates_public_requests_with_signed_cookie() {
        let config = LatitudeConfig::default();
        let state = AppState::new(PathBuf::from("latitude.test.json"), config.clone());
        let cookie = state.public_auth_cookie_value(&config.public_password);
        let req = Request::builder()
            .header(header::COOKIE, format!("{AUTH_COOKIE_NAME}={cookie}"))
            .body(Body::empty())
            .unwrap();

        assert!(public_request_is_authenticated(&state, &config, &req));

        let changed_config = LatitudeConfig {
            public_password: "changed".to_string(),
            ..config
        };
        assert!(!public_request_is_authenticated(
            &state,
            &changed_config,
            &req
        ));
    }

    #[test]
    fn authenticates_public_requests_with_bearer_token() {
        let config = LatitudeConfig::default();
        let state = AppState::new(PathBuf::from("latitude.test.json"), config.clone());
        let token = state.public_auth_cookie_value(&config.public_password);
        let req = Request::builder()
            .header(header::AUTHORIZATION, format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();

        assert!(public_request_is_authenticated(&state, &config, &req));
    }

    #[test]
    fn cleans_public_login_next_paths() {
        assert_eq!(
            clean_next_path(Some("/demo/_diff?x=1".to_string())),
            "/demo/_diff?x=1"
        );
        assert_eq!(
            clean_next_path(Some("https://example.com".to_string())),
            "/"
        );
        assert_eq!(clean_next_path(Some("//example.com".to_string())), "/");
        assert_eq!(clean_next_path(Some(LOGIN_PATH.to_string())), "/");
        assert_eq!(clean_next_path(Some("/demo name".to_string())), "/");
    }

    #[test]
    fn filters_public_auth_cookie_from_proxy_headers() {
        let value = HeaderValue::from_static("app=one; latitude_public_session=secret; theme=dark");

        assert_eq!(
            filtered_cookie_header(&value, AUTH_COOKIE_NAME).as_deref(),
            Some("app=one; theme=dark")
        );

        let value = HeaderValue::from_static("latitude_public_session=secret");
        assert_eq!(filtered_cookie_header(&value, AUTH_COOKIE_NAME), None);
    }

    #[test]
    fn matches_public_passwords_exactly() {
        assert!(public_password_matches("test", "test"));
        assert!(!public_password_matches("test", "Test"));
        assert!(!public_password_matches("test", "test "));
    }

    #[test]
    fn joins_upstream_url_with_query() {
        let joined = join_upstream_url("http://127.0.0.1:3000", "/hello", Some("a=1")).unwrap();
        assert_eq!(joined, "http://127.0.0.1:3000/hello?a=1");
    }

    #[test]
    fn joins_upstream_url_with_base_path() {
        let joined =
            join_upstream_url("http://127.0.0.1:3000/base/", "/hello", Some("a=1")).unwrap();
        assert_eq!(joined, "http://127.0.0.1:3000/base/hello?a=1");
    }

    #[test]
    fn parses_raw_markdown_page_payload() {
        let payload =
            parse_page_payload(Some("text/markdown; charset=utf-8"), b"# Agent Report").unwrap();

        assert_eq!(payload.format, PageFormat::Markdown);
        assert_eq!(payload.content, "# Agent Report");
        assert_eq!(payload.title, None);
    }

    #[test]
    fn parses_json_page_payload() {
        let payload = parse_page_payload(
            Some("application/json"),
            br##"{"title":"Report","format":"markdown","content":"# Done"}"##,
        )
        .unwrap();

        assert_eq!(payload.format, PageFormat::Markdown);
        assert_eq!(payload.content, "# Done");
        assert_eq!(payload.title.as_deref(), Some("Report"));
    }

    #[test]
    fn infers_html_for_raw_html_payload() {
        let payload = parse_page_payload(None, b"<section><h1>Hello</h1></section>").unwrap();

        assert_eq!(payload.format, PageFormat::Html);
    }

    #[test]
    fn renders_markdown_as_html_document() {
        let rendered = render_page_content(
            None,
            PageFormat::Markdown,
            "# Agent Report\n\n- Done",
            Some("dark"),
        );

        assert!(rendered.contains("<html lang=\"en\" data-latitude-theme=\"dark\">"));
        assert!(rendered.contains("<title>Agent Report</title>"));
        assert!(rendered.contains("<h1>Agent Report</h1>"));
        assert!(rendered.contains("<li>Done</li>"));
    }

    #[test]
    fn renders_project_home_with_enabled_deployments() {
        let rendered = render_project_home(&ProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: vec![
                ApplicationConfig {
                    name: "website".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Static {
                        root: PathBuf::from("."),
                        index_file: "index.html".to_string(),
                        spa_fallback: true,
                    },
                },
                ApplicationConfig {
                    name: "report".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        content: "# Report".to_string(),
                        format: PageFormat::Markdown,
                        title: Some("Weekly Report".to_string()),
                    },
                },
                ApplicationConfig {
                    name: "draft".to_string(),
                    enabled: false,
                    target: ApplicationTarget::Page {
                        content: "# Draft".to_string(),
                        format: PageFormat::Markdown,
                        title: None,
                    },
                },
            ],
        });

        assert!(rendered.contains("href=\"/demo/_diff\""));
        assert!(rendered.contains("Code changes"));
        assert!(rendered.contains("href=\"/demo/_terminal\""));
        assert!(rendered.contains("Run commands in the project directory"));
        assert!(rendered.contains("href=\"/demo/website\""));
        assert!(rendered.contains("Static website"));
        assert!(rendered.contains("href=\"/demo/report\""));
        assert!(rendered.contains("Page: Weekly Report"));
        assert!(!rendered.contains("/demo/draft"));
    }

    #[test]
    fn renders_project_diff_with_escaped_highlighted_lines() {
        let project = ProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: Vec::new(),
        };
        let report = GitDiffReport {
            repo_dir: PathBuf::from("C:/work/demo"),
            file_changes: vec![
                GitFileChange {
                    path: "src/server.rs".to_string(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                    diffs: vec![GitFileDiff {
                        label: "Unstaged".to_string(),
                        command: "git diff --no-ext-diff --color=never".to_string(),
                        path: "src/server.rs".to_string(),
                        content:
                            "diff --git a/src/server.rs b/src/server.rs\n@@ -1 +1 @@\n-let old = 1;\n+let new = 42;"
                                .to_string(),
                    }],
                },
                GitFileChange {
                    path: "src/new.rs".to_string(),
                    original_path: None,
                    index_status: 'A',
                    worktree_status: ' ',
                    diffs: Vec::new(),
                },
            ],
        };
        let rendered = render_project_diff(&project, &report);

        assert!(rendered.contains("<title>demo code changes - Latitude</title>"));
        assert!(rendered.contains("href=\"/demo\""));
        assert!(rendered.contains("<h2>Unstaged files</h2>"));
        assert!(rendered.contains("<h2>Staged files</h2>"));
        assert!(rendered.contains("data-diff-workspace"));
        assert!(rendered.contains("data-action-url=\"/demo/_diff\""));
        assert!(
            rendered.contains("<details class=\"file-card\" data-file-path=\"src/server.rs\">")
        );
        assert!(rendered.contains("<strong>Unstaged</strong>"));
        assert!(rendered.contains("data-git-action=\"stage_all\""));
        assert!(rendered.contains("data-git-action=\"stage_file\""));
        assert!(rendered.contains("data-path=\"src/server.rs\""));
        assert!(rendered.contains("data-git-action=\"unstage_file\""));
        assert!(rendered.contains("data-path=\"src/new.rs\""));
        assert!(rendered.contains("data-commit-message"));
        assert!(rendered.contains("Commit staged"));
        assert!(rendered.contains("method: 'PATCH'"));
        assert!(!rendered.contains("method=\"post\""));
        assert!(!rendered.contains("Done."));
        assert!(
            rendered.contains("class=\"line remove\">-<span class=\"tok-keyword\">let</span> old")
        );
        assert!(
            rendered.contains("class=\"line add\">+<span class=\"tok-keyword\">let</span> new")
        );
        assert!(rendered.contains("<span class=\"tok-number\">42</span>"));
        assert!(!rendered.contains("<h2>Git status</h2>"));
        assert!(!rendered.contains("<h2>Untracked files</h2>"));
    }

    #[test]
    fn renders_diff_workspace_fragment_without_full_document() {
        let report = GitDiffReport {
            repo_dir: PathBuf::from("C:/work/demo"),
            file_changes: vec![GitFileChange {
                path: "README.md".to_string(),
                original_path: None,
                index_status: '?',
                worktree_status: '?',
                diffs: Vec::new(),
            }],
        };

        let rendered = render_diff_workspace_fragment(&report);

        assert!(rendered.contains("data-action-status hidden"));
        assert!(rendered.contains("<h2>Unstaged files</h2>"));
        assert!(rendered.contains("data-git-action=\"stage_file\""));
        assert!(!rendered.contains("<!doctype html>"));
        assert!(!rendered.contains("<script>"));
    }

    #[test]
    fn parses_git_action_forms() {
        assert_eq!(
            parse_git_action_form(b"action=stage_all").unwrap(),
            GitAction::StageAll
        );
        assert_eq!(
            parse_git_action_form(b"action=stage_file&path=src%2Fserver.rs").unwrap(),
            GitAction::StageFile {
                path: "src/server.rs".to_string()
            }
        );
        assert_eq!(
            parse_git_action_form(b"action=unstage_all").unwrap(),
            GitAction::UnstageAll
        );
        assert_eq!(
            parse_git_action_form(b"action=unstage_file&path=src%5Cserver.rs").unwrap(),
            GitAction::UnstageFile {
                path: "src/server.rs".to_string()
            }
        );
        assert_eq!(
            parse_git_action_form(b"action=push").unwrap(),
            GitAction::Push
        );
        assert_eq!(
            parse_git_action_form(b"action=commit&message=Ship+diff+viewer").unwrap(),
            GitAction::Commit {
                message: "Ship diff viewer".to_string()
            }
        );
        assert!(parse_git_action_form(b"action=commit&message=%20").is_err());
        assert!(parse_git_action_form(b"action=wat").is_err());
    }

    #[test]
    fn parses_public_git_action_json_payloads() {
        assert_eq!(
            parse_public_git_action_payload(
                Some("application/json"),
                br#"{"action":"stage_file","path":"src\\server.rs"}"#,
            )
            .unwrap(),
            GitAction::StageFile {
                path: "src/server.rs".to_string()
            }
        );
        assert_eq!(
            parse_public_git_action_payload(
                Some("application/json; charset=utf-8"),
                br#"{"action":"commit","message":"Ship mobile app"}"#,
            )
            .unwrap(),
            GitAction::Commit {
                message: "Ship mobile app".to_string()
            }
        );
    }

    #[test]
    fn parses_terminal_command_payloads() {
        assert_eq!(
            parse_terminal_command_payload(
                Some("application/json"),
                br#"{"command":" cargo test "}"#,
            )
            .unwrap(),
            "cargo test"
        );
        assert_eq!(
            parse_terminal_command_payload(
                Some("application/x-www-form-urlencoded"),
                b"command=Get-ChildItem",
            )
            .unwrap(),
            "Get-ChildItem"
        );
        assert!(
            parse_terminal_command_payload(Some("application/json"), br#"{"command":" "}"#)
                .is_err()
        );
    }

    #[test]
    fn renders_project_terminal_page() {
        let project = ProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: Vec::new(),
        };
        let info = PublicTerminalInfoResponse {
            cwd: "C:/work/demo".to_string(),
            shell: "powershell",
            timeout_seconds: 30,
            max_output_bytes: 1024,
            sessions_href: "/__latitude/api/projects/demo/terminal/sessions".to_string(),
        };
        let rendered = render_project_terminal(&project, &info, Some("signed-token"));

        assert!(rendered.contains("<title>demo terminal - Latitude</title>"));
        assert!(rendered.contains("data-terminal-workspace"));
        assert!(
            rendered
                .contains("data-sessions-path=\"/__latitude/api/projects/demo/terminal/sessions\"")
        );
        assert!(rendered.contains("data-terminal-sessions"));
        assert!(rendered.contains("data-terminal-new"));
        assert!(rendered.contains("data-terminal-stack"));
        assert!(rendered.contains("data-ws-path=\"/demo/_terminal/ws\""));
        assert!(rendered.contains("data-ws-token=\"signed-token\""));
        assert!(rendered.contains("@xterm/xterm"));
        assert!(rendered.contains("C:/work/demo"));
    }

    #[test]
    fn parses_porcelain_status_for_file_actions() {
        let changes = parse_porcelain_status(
            b" M src/server.rs\0A  src/new.rs\0?? README.md\0R  src/new-name.rs\0src/old-name.rs\0",
        );

        assert_eq!(
            changes,
            vec![
                GitFileChange {
                    path: "src/server.rs".to_string(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                    diffs: Vec::new(),
                },
                GitFileChange {
                    path: "src/new.rs".to_string(),
                    original_path: None,
                    index_status: 'A',
                    worktree_status: ' ',
                    diffs: Vec::new(),
                },
                GitFileChange {
                    path: "README.md".to_string(),
                    original_path: None,
                    index_status: '?',
                    worktree_status: '?',
                    diffs: Vec::new(),
                },
                GitFileChange {
                    path: "src/new-name.rs".to_string(),
                    original_path: Some("src/old-name.rs".to_string()),
                    index_status: 'R',
                    worktree_status: ' ',
                    diffs: Vec::new(),
                },
            ]
        );
        assert!(changes[0].can_stage());
        assert!(!changes[0].can_unstage());
        assert!(!changes[1].can_stage());
        assert!(changes[1].can_unstage());
        assert!(changes[2].can_stage());
    }

    #[test]
    fn parses_combined_diff_into_file_sections() {
        let sections = parse_diff_file_sections(
            "Unstaged",
            "git diff",
            "diff --git a/src/a.rs b/src/a.rs\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/README.md b/README.md\n@@ -0,0 +1 @@\n+hi\n",
        );

        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].path, "src/a.rs");
        assert_eq!(sections[0].label, "Unstaged");
        assert!(sections[0].content.contains("+b"));
        assert_eq!(sections[1].path, "README.md");
        assert!(sections[1].content.contains("+hi"));
    }

    #[test]
    fn classifies_diff_lines() {
        assert_eq!(diff_line_class("diff --git a/a b/a"), Some("file"));
        assert_eq!(diff_line_class("@@ -1 +1 @@"), Some("hunk"));
        assert_eq!(diff_line_class("+added"), Some("add"));
        assert_eq!(diff_line_class("-removed"), Some("remove"));
        assert_eq!(diff_line_class(" context"), None);
    }

    #[test]
    fn highlights_diff_code_by_file_path() {
        let mut rendered = String::new();
        render_diff_code_output(
            &mut rendered,
            "diff --git a/src/lib.rs b/src/lib.rs\n@@ -0,0 +1 @@\n+pub fn answer() -> i32 { 42 }",
            "src/lib.rs",
        );

        assert!(rendered.contains("class=\"line file\">diff --git"));
        assert!(rendered.contains("class=\"line hunk\">@@ -0,0 +1 @@"));
        assert!(rendered.contains("+<span class=\"tok-keyword\">pub</span>"));
        assert!(rendered.contains("<span class=\"tok-keyword\">fn</span> answer"));
        assert!(rendered.contains("<span class=\"tok-type\">i32</span>"));
        assert!(rendered.contains("<span class=\"tok-number\">42</span>"));
    }

    #[test]
    fn detects_syntax_language_from_path() {
        assert_eq!(
            syntax_language_for_path("src/main.rs"),
            SyntaxLanguage::Rust
        );
        assert_eq!(
            syntax_language_for_path("sites/app/App.svelte"),
            SyntaxLanguage::JavaScript
        );
        assert_eq!(
            syntax_language_for_path("package.json"),
            SyntaxLanguage::Json
        );
        assert_eq!(
            syntax_language_for_path("latitude.example.json"),
            SyntaxLanguage::Json
        );
        assert_eq!(syntax_language_for_path("README.md"), SyntaxLanguage::Plain);
    }

    #[test]
    fn trims_windows_extended_path_prefix_for_display() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\work\demo")),
            r"C:\work\demo"
        );
    }

    #[test]
    fn renders_server_home_with_enabled_projects() {
        let rendered = render_server_home(&LatitudeConfig {
            projects: vec![
                ProjectConfig {
                    name: "mock".to_string(),
                    enabled: true,
                    project_dir: PathBuf::from("."),
                    deployments: vec![ApplicationConfig {
                        name: "website".to_string(),
                        enabled: true,
                        target: ApplicationTarget::Static {
                            root: PathBuf::from("."),
                            index_file: "index.html".to_string(),
                            spa_fallback: true,
                        },
                    }],
                },
                ProjectConfig {
                    name: "hidden".to_string(),
                    enabled: false,
                    project_dir: PathBuf::from("."),
                    deployments: Vec::new(),
                },
            ],
            ..LatitudeConfig::default()
        });

        assert!(rendered.contains("<title>Latitude Projects</title>"));
        assert!(rendered.contains("href=\"/mock\""));
        assert!(rendered.contains("1 deployment"));
        assert!(!rendered.contains("href=\"/hidden\""));
    }

    #[test]
    fn builds_public_project_detail_with_enabled_deployments() {
        let detail = public_project_detail(&ProjectConfig {
            name: "demo".to_string(),
            enabled: true,
            project_dir: PathBuf::from("."),
            deployments: vec![
                ApplicationConfig {
                    name: "website".to_string(),
                    enabled: true,
                    target: ApplicationTarget::ReverseProxy {
                        upstream: "http://127.0.0.1:3000".to_string(),
                        strip_prefix: true,
                    },
                },
                ApplicationConfig {
                    name: "report".to_string(),
                    enabled: true,
                    target: ApplicationTarget::Page {
                        content: "# Report".to_string(),
                        format: PageFormat::Markdown,
                        title: Some("Weekly Report".to_string()),
                    },
                },
                ApplicationConfig {
                    name: "draft".to_string(),
                    enabled: false,
                    target: ApplicationTarget::Static {
                        root: PathBuf::from("."),
                        index_file: "index.html".to_string(),
                        spa_fallback: false,
                    },
                },
            ],
        });

        assert_eq!(detail.name, "demo");
        assert_eq!(detail.deployment_count, 2);
        assert_eq!(detail.diff.api_href, "/__latitude/api/projects/demo/diff");
        assert_eq!(
            detail.terminal.api_href,
            "/__latitude/api/projects/demo/terminal"
        );
        assert_eq!(detail.deployments[0].kind, "reverse_proxy");
        assert_eq!(detail.deployments[1].kind, "page");
        assert_eq!(
            detail.deployments[1].title.as_deref(),
            Some("Weekly Report")
        );
    }

    #[test]
    fn serves_full_html_document_without_wrapping() {
        let html = "<!doctype html><html><head><title>X</title></head><body>Hi</body></html>";

        assert_eq!(
            render_page_content(None, PageFormat::Html, html, Some("dark")),
            html
        );
    }
}
