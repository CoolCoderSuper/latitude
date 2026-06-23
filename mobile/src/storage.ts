import * as SecureStore from 'expo-secure-store';

import type { SessionRecord } from './types';

const BASE_URL_KEY = 'latitude.baseUrl';
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

export async function loadSession(): Promise<SessionRecord | null> {
  const [baseUrl, token] = await Promise.all([
    getItem(BASE_URL_KEY),
    getItem(TOKEN_KEY),
  ]);

  if (!baseUrl || !token) {
    return null;
  }

  return { baseUrl, token };
}

export async function loadBaseUrl(): Promise<string | null> {
  return getItem(BASE_URL_KEY);
}

export async function saveBaseUrl(baseUrl: string): Promise<void> {
  await setItem(BASE_URL_KEY, baseUrl);
}

export async function saveSession(session: SessionRecord): Promise<void> {
  await Promise.all([
    setItem(BASE_URL_KEY, session.baseUrl),
    setItem(TOKEN_KEY, session.token),
  ]);
}

export async function clearSession(): Promise<void> {
  await deleteItem(TOKEN_KEY);
}

export async function loadThemeMode(): Promise<StoredThemeMode | null> {
  const mode = await getItem(THEME_MODE_KEY);
  return mode === 'light' || mode === 'dark' ? mode : null;
}

export async function saveThemeMode(mode: StoredThemeMode): Promise<void> {
  await setItem(THEME_MODE_KEY, mode);
}
