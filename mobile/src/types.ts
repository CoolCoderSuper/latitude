export type SessionRecord = {
  baseUrl: string;
  token: string;
};

export type LoginResponse = {
  token: string;
  max_age_seconds: number;
  projects_href: string;
};

export type SessionResponse = {
  authenticated: boolean;
  projects_href: string | null;
};

export type ProjectSummary = {
  name: string;
  href: string;
  api_href: string;
  summary: string;
  deployment_count: number;
};

export type ProjectListResponse = {
  projects: ProjectSummary[];
};

export type DeploymentKind = 'reverse_proxy' | 'static' | 'page';

export type DeploymentSummary = {
  name: string;
  href: string;
  kind: DeploymentKind;
  label: string;
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
  href: string;
  api_href: string;
  summary: string;
  deployment_count: number;
  diff: ProjectDiffLink;
  terminal: ProjectTerminalLink;
  deployments: DeploymentSummary[];
};

export type GitFileDiff = {
  label: 'Unstaged' | 'Staged' | 'Untracked' | string;
  command: string;
  path: string;
  content: string;
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
