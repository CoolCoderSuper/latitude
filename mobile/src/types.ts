export type SessionRecord = {
  baseUrl: string;
  token: string;
  deviceHostname?: string;
};

export type LoginResponse = {
  token: string;
  max_age_seconds: number;
  projects_href: string;
  device_hostname: string;
};

export type SessionResponse = {
  authenticated: boolean;
  projects_href: string | null;
  device_hostname: string;
};

export type ProjectSummary = {
  name: string;
  href: string;
  api_href: string;
  summary: string;
  deployment_count: number;
};

export type ProjectListResponse = {
  device_hostname: string;
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

export type ProjectDiffLink = {
  href: string;
  api_href: string;
  label: string;
  description: string;
};

export type ProjectTerminalLink = {
  href: string;
  api_href: string;
  label: string;
  description: string;
};

export type ProjectDetail = {
  name: string;
  device_hostname: string;
  href: string;
  api_href: string;
  summary: string;
  deployment_count: number;
  diff: ProjectDiffLink;
  terminal: ProjectTerminalLink;
  deployments: DeploymentSummary[];
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
  file_changes: GitFileChange[];
};

export type GitActionName =
  | 'stage_all'
  | 'stage_file'
  | 'unstage_all'
  | 'unstage_file'
  | 'discard_all'
  | 'discard_file'
  | 'commit'
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

export type TerminalSessionSummary = {
  id: string;
  project: string;
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
