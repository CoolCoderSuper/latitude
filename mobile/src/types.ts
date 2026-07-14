export type SessionRecord = {
  baseUrl: string;
  token: string;
  deviceHostname?: string;
};

export type LoginResponse = {
  token: string;
  max_age_seconds: number;
  projects_href: string;
  root_terminal: RootTerminalLink;
  root_desktop: RootDesktopLink | null;
  device_hostname: string;
};

export type SessionResponse = {
  authenticated: boolean;
  projects_href: string | null;
  root_terminal: RootTerminalLink | null;
  root_desktop: RootDesktopLink | null;
  device_hostname: string;
};

export type ProjectSummary = {
  name: string;
  href: string;
  api_href: string;
  summary: string;
  deployment_count: number;
  git_dirty: boolean;
  git_additions: number;
  git_deletions: number;
  git_ahead: number;
  git_behind: number;
  worktree: WorktreeSummary | null;
};

export type WorktreeSummary = {
  repository: string;
  path: string;
  branch: string | null;
  head: string;
  discovered: boolean;
  archived: boolean;
};

export type ProjectListResponse = {
  device_hostname: string;
  root_terminal: RootTerminalLink;
  root_desktop: RootDesktopLink | null;
  projects: ProjectSummary[];
};

export type DeploymentKind = 'reverse_proxy' | 'static' | 'page';

export type DeploymentSummary = {
  name: string;
  href: string;
  kind: DeploymentKind;
  label: string;
  media_type: string | null;
  title: string | null;
};

export type DeploymentShare = {
  token: string;
  project: string;
  deployment: string;
  href: string;
  has_password: boolean;
  expires_at: number | null;
  expired: boolean;
};

export type CreateDeploymentSharePayload = {
  project: string;
  deployment: string;
  password?: string;
  expires_at?: number;
};

export type ProjectDiffLink = {
  href: string;
  api_href: string;
  label: string;
  description: string;
};

export type TerminalLink = {
  href: string;
  api_href: string;
  label: string;
  description: string;
};

export type ProjectTerminalLink = TerminalLink;
export type RootTerminalLink = TerminalLink;
export type RootDesktopLink = TerminalLink & {
  view_only: boolean;
  screens?: DesktopScreen[];
};

export type DesktopScreen = {
  id: string;
  label: string;
  title: string;
  x: number;
  y: number;
  width: number;
  height: number;
  primary: boolean;
};

export type ProjectDetail = {
  name: string;
  device_hostname: string;
  href: string;
  api_href: string;
  summary: string;
  deployment_count: number;
  git_dirty: boolean;
  git_additions: number;
  git_deletions: number;
  git_ahead: number;
  git_behind: number;
  diff: ProjectDiffLink;
  terminal: ProjectTerminalLink;
  deployments: DeploymentSummary[];
};

export type ProjectFileEntry = {
  name: string;
  path: string;
  kind: 'directory' | 'file';
  size: number;
};

export type ProjectDirectoryResponse = {
  path: string;
  entries: ProjectFileEntry[];
};

export type DiffLineKind = 'file' | 'hunk' | 'add' | 'remove';

export type DiffTokenKind =
  | 'comment'
  | 'keyword'
  | 'number'
  | 'property'
  | 'punctuation'
  | 'string'
  | 'type';

export type DiffToken = {
  text: string;
  kind?: DiffTokenKind;
};

export type DiffLine = {
  kind?: DiffLineKind;
  tokens: DiffToken[];
};

export type GitFileDiff = {
  label: 'Unstaged' | 'Staged' | 'Untracked' | string;
  command: string;
  path: string;
  content: string;
  lines?: DiffLine[];
};

export type GitFileChange = {
  path: string;
  original_path: string | null;
  index_status: string;
  worktree_status: string;
  diffs: GitFileDiff[];
};

export type GitDiffResponse = {
  repo_dir: string;
  unstaged_count: number;
  staged_count: number;
  additions: number;
  deletions: number;
  ahead: number;
  behind: number;
  file_changes: GitFileChange[];
};

export type GitCommitSummary = {
  hash: string;
  short_hash: string;
  author: string;
  authored_at: string;
  subject: string;
};

export type GitHistoryResponse = {
  repo_dir: string;
  commits: GitCommitSummary[];
};

export type GitCommitResponse = GitCommitSummary & {
  repo_dir: string;
  additions: number;
  deletions: number;
  files: GitFileDiff[];
};

export type GitActionName =
  | 'stage_all'
  | 'stage_file'
  | 'unstage_all'
  | 'unstage_file'
  | 'discard_all'
  | 'discard_file'
  | 'commit'
  | 'fetch'
  | 'pull'
  | 'push';

export type GitActionPayload = {
  action: GitActionName;
  path?: string;
  message?: string;
};

export type GitActionResponse = {
  ok: boolean;
  error: string | null;
  diff: GitDiffResponse;
};

export type TerminalInfoResponse = {
  cwd: string;
  shell: string;
  timeout_seconds: number;
  max_output_bytes: number;
  sessions_href: string;
};

export type DesktopInfoResponse = {
  label: string;
  enabled: boolean;
  mode: 'external' | 'managed';
  managed: boolean;
  host: string;
  port: number;
  view_only: boolean;
  websocket_href: string;
  screens: DesktopScreen[];
};

export type TerminalSessionSummary = {
  id: string;
  scope: string;
  project?: string | null;
  title: string;
  cwd: string;
  created_at_ms: number;
  connected_clients: number;
  alive: boolean;
};

export type TerminalSessionListResponse = {
  sessions: TerminalSessionSummary[];
};

export type TerminalCommandPayload = {
  command: string;
};

export type TerminalCommandResponse = {
  command: string;
  cwd: string;
  shell: string;
  exit_code: number | null;
  success: boolean;
  stdout: string;
  stderr: string;
  duration_ms: number;
  timed_out: boolean;
};
