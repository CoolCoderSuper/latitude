import type {
  DesktopInfoResponse,
  CreateDeploymentSharePayload,
  DeploymentShare,
  GitActionPayload,
  GitActionResponse,
  GitCommitResponse,
  GitDiffResponse,
  GitHistoryResponse,
  LoginResponse,
  ProjectDetail,
  ProjectDirectoryResponse,
  ProjectFileSearchKind,
  ProjectFileSearchResponse,
  ProjectListResponse,
  SessionResponse,
  TerminalCommandPayload,
  TerminalCommandResponse,
  TerminalInfoResponse,
  TerminalSessionListResponse,
  TerminalSessionSummary,
} from './types';

const PUBLIC_API_PREFIX = '/__latitude/api';
const REQUEST_TIMEOUT_MS = 45_000;

export class LatitudeApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = 'LatitudeApiError';
    this.status = status;
  }
}

export class LatitudeRequestCancelledError extends Error {
  constructor() {
    super('Latitude request was cancelled.');
    this.name = 'LatitudeRequestCancelledError';
  }
}

export function normalizeBaseUrl(value: string): string {
  const trimmed = value.trim().replace(/\/+$/, '');
  if (!trimmed) {
    return '';
  }
  if (/^https?:\/\//i.test(trimmed)) {
    return trimmed;
  }

  return `http://${trimmed}`;
}

export function absoluteUrl(baseUrl: string, href: string): string {
  return new URL(href, `${normalizeBaseUrl(baseUrl)}/`).toString();
}

export function authHeaders(token: string): Record<string, string> {
  return {
    Authorization: `Bearer ${token}`,
    Cookie: `latitude_public_session=${token}`,
  };
}

export class LatitudePublicApi {
  private baseUrl: string;
  private token?: string;

  constructor(baseUrl: string, token?: string) {
    this.baseUrl = normalizeBaseUrl(baseUrl);
    this.token = token;
  }

  setSession(baseUrl: string, token?: string) {
    this.baseUrl = normalizeBaseUrl(baseUrl);
    this.token = token;
  }

  async session(): Promise<SessionResponse> {
    return this.get<SessionResponse>(`${PUBLIC_API_PREFIX}/session`, false);
  }

  async login(password: string): Promise<LoginResponse> {
    return this.request<LoginResponse>(`${PUBLIC_API_PREFIX}/session`, {
      method: 'POST',
      body: JSON.stringify({ password }),
      headers: {
        'Content-Type': 'application/json',
      },
      includeAuth: false,
    });
  }

  async projects(
    fetchRemote = false,
    autoRefresh = false,
    signal?: AbortSignal,
  ): Promise<ProjectListResponse> {
    const query = refreshQuery(fetchRemote, autoRefresh);
    return this.get<ProjectListResponse>(
      `${PUBLIC_API_PREFIX}/projects${query}`,
      true,
      signal,
    );
  }

  async project(
    name: string,
    fetchRemote = false,
    autoRefresh = false,
    signal?: AbortSignal,
  ): Promise<ProjectDetail> {
    const query = refreshQuery(fetchRemote, autoRefresh);
    return this.get<ProjectDetail>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(name)}${query}`,
      true,
      signal,
    );
  }

  async setWorktreeArchived(name: string, archived: boolean): Promise<void> {
    await this.request(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(name)}/archive`,
      {
        method: 'PATCH',
        body: JSON.stringify({ archived }),
        headers: { 'Content-Type': 'application/json' },
      },
    );
  }

  async setDeploymentArchived(
    projectName: string,
    deploymentName: string,
    archived: boolean,
  ): Promise<void> {
    await this.request(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/deployments/${encodeURIComponent(deploymentName)}/archive`,
      {
        method: 'PATCH',
        body: JSON.stringify({ archived }),
        headers: { 'Content-Type': 'application/json' },
      },
    );
  }

  async shares(): Promise<DeploymentShare[]> {
    return this.get<DeploymentShare[]>(`${PUBLIC_API_PREFIX}/shares`);
  }

  async createShare(
    payload: CreateDeploymentSharePayload,
  ): Promise<DeploymentShare> {
    return this.request<DeploymentShare>(`${PUBLIC_API_PREFIX}/shares`, {
      method: 'POST',
      body: JSON.stringify(payload),
      headers: {
        'Content-Type': 'application/json',
      },
    });
  }

  async deleteShare(token: string): Promise<void> {
    await this.request<void>(
      `${PUBLIC_API_PREFIX}/shares/${encodeURIComponent(token)}`,
      { method: 'DELETE' },
    );
  }

  async diff(
    projectName: string,
    signal?: AbortSignal,
  ): Promise<GitDiffResponse> {
    return this.get<GitDiffResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/diff`,
      true,
      signal,
    );
  }

  async gitHistory(projectName: string): Promise<GitHistoryResponse> {
    return this.get<GitHistoryResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/diff/history`,
    );
  }

  async gitCommit(
    projectName: string,
    hash: string,
  ): Promise<GitCommitResponse> {
    return this.get<GitCommitResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/diff/history/${encodeURIComponent(hash)}`,
    );
  }

  async files(
    projectName: string,
    path = '',
    signal?: AbortSignal,
  ): Promise<ProjectDirectoryResponse> {
    return this.get<ProjectDirectoryResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/files?path=${encodeURIComponent(path)}`,
      true,
      signal,
    );
  }

  async searchFiles(
    projectName: string,
    search: string,
    searchKind: ProjectFileSearchKind,
    signal?: AbortSignal,
  ): Promise<ProjectFileSearchResponse> {
    const params = new URLSearchParams({
      search,
      search_kind: searchKind,
    });
    return this.get<ProjectFileSearchResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/files?${params.toString()}`,
      true,
      signal,
    );
  }

  async runGitAction(
    projectName: string,
    payload: GitActionPayload,
  ): Promise<GitActionResponse> {
    return this.request<GitActionResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/diff`,
      {
        method: 'PATCH',
        body: JSON.stringify(payload),
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );
  }

  async terminal(projectName: string): Promise<TerminalInfoResponse> {
    return this.get<TerminalInfoResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/terminal`,
    );
  }

  async rootTerminal(): Promise<TerminalInfoResponse> {
    return this.get<TerminalInfoResponse>(`${PUBLIC_API_PREFIX}/terminal`);
  }

  async rootDesktop(): Promise<DesktopInfoResponse> {
    return this.get<DesktopInfoResponse>(`${PUBLIC_API_PREFIX}/desktop`);
  }

  async runTerminalCommand(
    projectName: string,
    payload: TerminalCommandPayload,
  ): Promise<TerminalCommandResponse> {
    return this.request<TerminalCommandResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/terminal`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );
  }

  async runRootTerminalCommand(
    payload: TerminalCommandPayload,
  ): Promise<TerminalCommandResponse> {
    return this.request<TerminalCommandResponse>(
      `${PUBLIC_API_PREFIX}/terminal`,
      {
        method: 'POST',
        body: JSON.stringify(payload),
        headers: {
          'Content-Type': 'application/json',
        },
      },
    );
  }

  async terminalSessions(
    projectName: string,
  ): Promise<TerminalSessionListResponse> {
    return this.get<TerminalSessionListResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/terminal/sessions`,
    );
  }

  async rootTerminalSessions(): Promise<TerminalSessionListResponse> {
    return this.get<TerminalSessionListResponse>(
      `${PUBLIC_API_PREFIX}/terminal/sessions`,
    );
  }

  async createTerminalSession(
    projectName: string,
  ): Promise<TerminalSessionSummary> {
    return this.request<TerminalSessionSummary>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/terminal/sessions`,
      { method: 'POST' },
    );
  }

  async createRootTerminalSession(): Promise<TerminalSessionSummary> {
    return this.request<TerminalSessionSummary>(
      `${PUBLIC_API_PREFIX}/terminal/sessions`,
      { method: 'POST' },
    );
  }

  async closeTerminalSession(
    projectName: string,
    sessionId: string,
  ): Promise<void> {
    await this.request<void>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/terminal/sessions/${encodeURIComponent(sessionId)}`,
      { method: 'DELETE' },
    );
  }

  async closeRootTerminalSession(sessionId: string): Promise<void> {
    await this.request<void>(
      `${PUBLIC_API_PREFIX}/terminal/sessions/${encodeURIComponent(sessionId)}`,
      { method: 'DELETE' },
    );
  }

  private async get<T>(
    path: string,
    includeAuth = true,
    signal?: AbortSignal,
  ): Promise<T> {
    return this.request<T>(path, { method: 'GET', includeAuth, signal });
  }

  private async request<T>(
    path: string,
    options: RequestInit & { includeAuth?: boolean } = {},
  ): Promise<T> {
    if (!this.baseUrl) {
      throw new LatitudeApiError(0, 'Latitude URL is required.');
    }

    const {
      includeAuth = true,
      signal: externalSignal,
      ...requestOptions
    } = options;
    const headers: Record<string, string> = {
      Accept: 'application/json',
      ...(options.headers as Record<string, string> | undefined),
    };

    if (includeAuth && this.token) {
      Object.assign(headers, authHeaders(this.token));
    }

    const url = absoluteUrl(this.baseUrl, path);
    const controller = new AbortController();
    let timedOut = false;
    const abortFromCaller = () => controller.abort();
    if (externalSignal?.aborted) {
      controller.abort();
    } else {
      externalSignal?.addEventListener('abort', abortFromCaller, {
        once: true,
      });
    }
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, REQUEST_TIMEOUT_MS);

    try {
      const response = await fetch(url, {
        ...requestOptions,
        headers,
        signal: controller.signal,
      });
      const payload = await response.json().catch(() => null);
      if (!response.ok) {
        throw new LatitudeApiError(
          response.status,
          payload && typeof payload.error === 'string'
            ? payload.error
            : `Latitude returned ${response.status}.`,
        );
      }
      return payload as T;
    } catch (error) {
      if (error instanceof LatitudeApiError) {
        throw error;
      }
      if (timedOut) {
        throw new LatitudeApiError(
          0,
          `Latitude did not respond within ${REQUEST_TIMEOUT_MS / 1000} seconds.`,
        );
      }
      if (externalSignal?.aborted || controller.signal.aborted) {
        throw new LatitudeRequestCancelledError();
      }
      const reason =
        error instanceof Error ? error.message : 'Could not reach Latitude.';
      throw new LatitudeApiError(
        0,
        `Could not reach ${this.baseUrl}. ${reason}`,
      );
    } finally {
      clearTimeout(timeout);
      externalSignal?.removeEventListener('abort', abortFromCaller);
    }
  }
}

function refreshQuery(fetchRemote: boolean, autoRefresh: boolean): string {
  const params = new URLSearchParams();
  if (fetchRemote) params.set('fetch', '1');
  if (autoRefresh) params.set('refresh', 'auto');
  const query = params.toString();
  return query ? `?${query}` : '';
}
