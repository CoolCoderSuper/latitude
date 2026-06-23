import * as SecureStore from 'expo-secure-store';

import type { SessionRecord } from './types';

const BASE_URL_KEY = 'latitude.baseUrl';
const ACTIVE_BASE_URL_KEY = 'latitude.activeBaseUrl';
const SESSIONS_KEY = 'latitude.sessions';
const THEME_MODE_KEY = 'latitude.themeMode';
const TOKEN_KEY = 'latitude.token';

export type StoredThemeMode = 'light' | 'dark';

const fallbackStore: Record<string, string | undefined> = {};

async function secureStoreAvailable(): Promise<boolean> {
  try {
    return await SecureStore.isAvailableAsync();
  } catch {
    return false;
  }
}

async function getItem(key: string): Promise<string | null> {
  if (await secureStoreAvailable()) {
    return SecureStore.getItemAsync(key);
  }

  return fallbackStore[key] ?? null;
}

async function setItem(key: string, value: string): Promise<void> {
  if (await secureStoreAvailable()) {
    await SecureStore.setItemAsync(key, value);
    return;
  }

  fallbackStore[key] = value;
}

async function deleteItem(key: string): Promise<void> {
  if (await secureStoreAvailable()) {
    await SecureStore.deleteItemAsync(key);
    return;
  }

  delete fallbackStore[key];
}

function normalizeStoredBaseUrl(baseUrl: string): string {
  return baseUrl.trim().replace(/\/+$/, '');
}

function sanitizeSession(value: unknown): SessionRecord | null {
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

  return { baseUrl, token };
}

function mergeSessions(sessions: SessionRecord[]): SessionRecord[] {
  const byBaseUrl = new Map<string, SessionRecord>();

  for (const session of sessions) {
    byBaseUrl.set(session.baseUrl, session);
  }

  return Array.from(byBaseUrl.values());
}

function parseSessions(rawSessions: string | null): SessionRecord[] {
  if (!rawSessions) {
    return [];
  }

  try {
    const parsed = JSON.parse(rawSessions);
    if (!Array.isArray(parsed)) {
      return [];
    }

    return mergeSessions(
      parsed
        .map((item) => sanitizeSession(item))
        .filter((item): item is SessionRecord => Boolean(item)),
    );
  } catch {
    return [];
  }
}

async function loadLegacySession(): Promise<SessionRecord | null> {
  const [baseUrl, token] = await Promise.all([
    getItem(BASE_URL_KEY),
    getItem(TOKEN_KEY),
  ]);

  if (!baseUrl || !token) {
    return null;
  }

  return sanitizeSession({ baseUrl, token });
}

async function saveSessionList(sessions: SessionRecord[]): Promise<void> {
  await setItem(SESSIONS_KEY, JSON.stringify(mergeSessions(sessions)));
}

async function saveActiveSession(session: SessionRecord | null): Promise<void> {
  if (!session) {
    await Promise.all([
      deleteItem(ACTIVE_BASE_URL_KEY),
      deleteItem(TOKEN_KEY),
    ]);
    return;
  }

  await Promise.all([
    setItem(ACTIVE_BASE_URL_KEY, session.baseUrl),
    setItem(BASE_URL_KEY, session.baseUrl),
    setItem(TOKEN_KEY, session.token),
  ]);
}

export async function loadSessions(): Promise<SessionRecord[]> {
  const [rawSessions, legacySession] = await Promise.all([
    getItem(SESSIONS_KEY),
    loadLegacySession(),
  ]);

  return mergeSessions([
    ...parseSessions(rawSessions),
    ...(legacySession ? [legacySession] : []),
  ]);
}

export async function loadSession(): Promise<SessionRecord | null> {
  const [sessions, activeBaseUrl, rememberedBaseUrl] = await Promise.all([
    loadSessions(),
    getItem(ACTIVE_BASE_URL_KEY),
    getItem(BASE_URL_KEY),
  ]);

  if (sessions.length === 0) {
    return null;
  }

  const preferredBaseUrl = normalizeStoredBaseUrl(
    activeBaseUrl ?? rememberedBaseUrl ?? '',
  );
  return (
    sessions.find((item) => item.baseUrl === preferredBaseUrl) ??
    sessions[0] ??
    null
  );
}

export async function loadBaseUrl(): Promise<string | null> {
  return getItem(BASE_URL_KEY);
}

export async function saveBaseUrl(baseUrl: string): Promise<void> {
  await setItem(BASE_URL_KEY, normalizeStoredBaseUrl(baseUrl));
}

export async function saveSession(session: SessionRecord): Promise<SessionRecord[]> {
  const normalizedSession = sanitizeSession(session);
  if (!normalizedSession) {
    return loadSessions();
  }

  const sessions = mergeSessions([
    ...(await loadSessions()).filter(
      (item) => item.baseUrl !== normalizedSession.baseUrl,
    ),
    normalizedSession,
  ]);

  await Promise.all([
    saveSessionList(sessions),
    saveActiveSession(normalizedSession),
  ]);
  return sessions;
}

export async function activateSession(
  baseUrl: string,
): Promise<SessionRecord | null> {
  const sessions = await loadSessions();
  const normalizedBaseUrl = normalizeStoredBaseUrl(baseUrl);
  const session =
    sessions.find((item) => item.baseUrl === normalizedBaseUrl) ?? null;

  await saveActiveSession(session);
  return session;
}

export async function removeSession(baseUrl: string): Promise<{
  activeSession: SessionRecord | null;
  sessions: SessionRecord[];
}> {
  const [sessions, currentSession] = await Promise.all([
    loadSessions(),
    loadSession(),
  ]);
  const normalizedBaseUrl = normalizeStoredBaseUrl(baseUrl);
  const remainingSessions = sessions.filter(
    (item) => item.baseUrl !== normalizedBaseUrl,
  );
  const currentSessionStillSaved =
    currentSession &&
    remainingSessions.some((item) => item.baseUrl === currentSession.baseUrl);
  const activeSession = currentSessionStillSaved
    ? currentSession
    : remainingSessions[0] ?? null;

  await Promise.all([
    saveSessionList(remainingSessions),
    saveActiveSession(activeSession),
  ]);
  if (!activeSession) {
    await setItem(BASE_URL_KEY, normalizedBaseUrl);
  }

  return { activeSession, sessions: remainingSessions };
}

export async function clearSession(): Promise<void> {
  const session = await loadSession();
  if (session) {
    await removeSession(session.baseUrl);
    return;
  }

  await deleteItem(TOKEN_KEY);
}

export async function loadThemeMode(): Promise<StoredThemeMode | null> {
  const mode = await getItem(THEME_MODE_KEY);
  return mode === 'light' || mode === 'dark' ? mode : null;
}

export async function saveThemeMode(mode: StoredThemeMode): Promise<void> {
  await setItem(THEME_MODE_KEY, mode);
}
