import type {
  DesktopInfoResponse,
  GitActionPayload,
  GitActionResponse,
  GitDiffResponse,
  LoginResponse,
  ProjectDetail,
  ProjectListResponse,
  SessionResponse,
  TerminalCommandPayload,
  TerminalCommandResponse,
  TerminalInfoResponse,
  TerminalSessionListResponse,
  TerminalSessionSummary,
} from './types';

const PUBLIC_API_PREFIX = '/__latitude/api';

export class LatitudeApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = 'LatitudeApiError';
    this.status = status;
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

  async projects(): Promise<ProjectListResponse> {
    return this.get<ProjectListResponse>(`${PUBLIC_API_PREFIX}/projects`);
  }

  async project(name: string): Promise<ProjectDetail> {
    return this.get<ProjectDetail>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(name)}`,
    );
  }

  async diff(projectName: string): Promise<GitDiffResponse> {
    return this.get<GitDiffResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/diff`,
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

  async terminalSessions(projectName: string): Promise<TerminalSessionListResponse> {
    return this.get<TerminalSessionListResponse>(
      `${PUBLIC_API_PREFIX}/projects/${encodeURIComponent(projectName)}/terminal/sessions`,
    );
  }

  async rootTerminalSessions(): Promise<TerminalSessionListResponse> {
    return this.get<TerminalSessionListResponse>(
      `${PUBLIC_API_PREFIX}/terminal/sessions`,
    );
  }

  async createTerminalSession(projectName: string): Promise<TerminalSessionSummary> {
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

  async closeTerminalSession(projectName: string, sessionId: string): Promise<void> {
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

  private async get<T>(path: string, includeAuth = true): Promise<T> {
    return this.request<T>(path, { method: 'GET', includeAuth });
  }

  private async request<T>(
    path: string,
    options: RequestInit & { includeAuth?: boolean } = {},
  ): Promise<T> {
    if (!this.baseUrl) {
      throw new LatitudeApiError(0, 'Latitude URL is required.');
    }

    const includeAuth = options.includeAuth ?? true;
    const headers: Record<string, string> = {
      Accept: 'application/json',
      ...(options.headers as Record<string, string> | undefined),
    };

    if (includeAuth && this.token) {
      Object.assign(headers, authHeaders(this.token));
    }

    const url = absoluteUrl(this.baseUrl, path);
    let response: Response;
    try {
      response = await fetch(url, {
        ...options,
        headers,
      });
    } catch (error) {
      const reason = error instanceof Error ? error.message : 'Could not reach Latitude.';
      throw new LatitudeApiError(
        0,
        `Could not reach ${this.baseUrl}. ${reason}`,
      );
    }

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
  }
}
