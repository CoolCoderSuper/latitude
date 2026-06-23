pub(super) const AUTH_PAGE_STYLE: &str = r#"
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
pub(super) const PROJECT_HOME_STYLE: &str = r#"
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
pub(super) const DIFF_VIEWER_STYLE: &str = r#"
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
pub(super) const DIFF_VIEWER_SCRIPT: &str = r#"
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
pub(super) const TERMINAL_VIEWER_STYLE: &str = r#"
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
pub(super) const TERMINAL_VIEWER_SCRIPT: &str = r##"
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
pub(super) const PAGE_STYLE: &str = r#"
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
