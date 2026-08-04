import type { SessionRecord } from '../../types';

export function normalizeStoredBaseUrl(baseUrl: string): string {
  return baseUrl.trim().replace(/\/+$/, '');
}

function normalizeStoredHostname(hostname: unknown): string | undefined {
  if (typeof hostname !== 'string') {
    return undefined;
  }
  const normalized = hostname.trim();
  return normalized || undefined;
}

export function sanitizeSession(value: unknown): SessionRecord | null {
  if (!value || typeof value !== 'object') {
    return null;
  }

  const candidate = value as Partial<SessionRecord>;
  if (
    typeof candidate.baseUrl !== 'string' ||
    typeof candidate.token !== 'string'
  ) {
    return null;
  }

  const baseUrl = normalizeStoredBaseUrl(candidate.baseUrl);
  const token = candidate.token.trim();
  if (!baseUrl || !token) {
    return null;
  }

  const deviceHostname = normalizeStoredHostname(candidate.deviceHostname);
  return {
    baseUrl,
    token,
    ...(deviceHostname ? { deviceHostname } : {}),
  };
}

export function mergeSessions(sessions: SessionRecord[]): SessionRecord[] {
  const byBaseUrl = new Map<string, SessionRecord>();
  for (const session of sessions) {
    byBaseUrl.set(session.baseUrl, session);
  }
  return Array.from(byBaseUrl.values());
}

export function upsertSession(
  sessions: SessionRecord[],
  session: SessionRecord,
): SessionRecord[] {
  let replaced = false;
  const nextSessions = sessions.flatMap((item) => {
    if (item.baseUrl !== session.baseUrl) {
      return [item];
    }
    if (replaced) {
      return [];
    }
    replaced = true;
    return [session];
  });
  return replaced ? nextSessions : [...nextSessions, session];
}

export function parseSessions(rawSessions: string | null): SessionRecord[] {
  if (!rawSessions) {
    return [];
  }
  try {
    const parsed: unknown = JSON.parse(rawSessions);
    if (!Array.isArray(parsed)) {
      return [];
    }
    return mergeSessions(
      parsed
        .map(sanitizeSession)
        .filter((item): item is SessionRecord => Boolean(item)),
    );
  } catch {
    return [];
  }
}

export function sessionLabel(session: SessionRecord): string {
  const hostname = session.deviceHostname?.trim();
  if (hostname) {
    return hostname;
  }
  try {
    return new URL(session.baseUrl).host;
  } catch {
    return session.baseUrl;
  }
}

export function reorderSessions(
  sessions: SessionRecord[],
  sourceIndex: number,
  targetIndex: number,
): SessionRecord[] {
  if (
    sourceIndex < 0 ||
    targetIndex < 0 ||
    sourceIndex >= sessions.length ||
    targetIndex >= sessions.length
  ) {
    return sessions;
  }
  const reorderedSessions = [...sessions];
  const [session] = reorderedSessions.splice(sourceIndex, 1);
  reorderedSessions.splice(targetIndex, 0, session);
  return reorderedSessions;
}
