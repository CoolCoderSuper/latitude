import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from 'react';
import type { ReactNode } from 'react';

import { LatitudePublicApi, normalizeBaseUrl } from '../../api';
import { DEFAULT_BASE_URL } from '../../constants';
import {
  activateSession,
  loadBaseUrl,
  loadSession,
  loadSessions,
  removeSession,
  requireSessionLogin,
  saveBaseUrl,
  saveSession,
  saveSessionOrder,
} from '../../storage';
import type { SessionRecord } from '../../types';
import { errorMessage } from '../../utils/errors';

type SessionContextValue = {
  booting: boolean;
  clearError: () => void;
  error: string | null;
  expireSession: (session: SessionRecord, message?: string) => Promise<void>;
  login: (baseUrl: string, password: string) => Promise<void>;
  rememberedBaseUrl: string;
  removeServer: (baseUrl: string) => Promise<void>;
  reorderServers: (sessions: SessionRecord[]) => Promise<void>;
  session: SessionRecord | null;
  sessions: SessionRecord[];
  switchServer: (baseUrl: string) => Promise<void>;
  updateDeviceHostname: (baseUrl: string, hostname: string) => Promise<void>;
};

const SessionContext = createContext<SessionContextValue | null>(null);

export function SessionProvider({ children }: { children: ReactNode }) {
  const [booting, setBooting] = useState(true);
  const [rememberedBaseUrl, setRememberedBaseUrl] = useState(DEFAULT_BASE_URL);
  const [session, setSession] = useState<SessionRecord | null>(null);
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;

    Promise.all([loadSession(), loadBaseUrl(), loadSessions()])
      .then(([storedSession, storedBaseUrl, storedSessions]) => {
        if (!mounted) {
          return;
        }
        setSessions(storedSessions);
        setRememberedBaseUrl(
          storedSession?.baseUrl ?? storedBaseUrl ?? DEFAULT_BASE_URL,
        );
        setSession(storedSession);
      })
      .catch((storageError) => {
        if (mounted) {
          setError(errorMessage(storageError));
        }
      })
      .finally(() => {
        if (mounted) {
          setBooting(false);
        }
      });

    return () => {
      mounted = false;
    };
  }, []);

  const clearError = useCallback(() => setError(null), []);

  const login = useCallback(async (baseUrl: string, password: string) => {
    const normalizedBaseUrl = normalizeBaseUrl(baseUrl);
    await saveBaseUrl(normalizedBaseUrl);
    setRememberedBaseUrl(normalizedBaseUrl);

    const response = await new LatitudePublicApi(normalizedBaseUrl).login(
      password,
    );
    const nextSession = {
      baseUrl: normalizedBaseUrl,
      token: response.token,
      deviceHostname: response.device_hostname,
    };
    setSessions(await saveSession(nextSession));
    setSession(nextSession);
    setError(null);
  }, []);

  const switchServer = useCallback(async (baseUrl: string) => {
    const nextSession = await activateSession(baseUrl);
    if (!nextSession) {
      return;
    }
    setSession(nextSession);
    setRememberedBaseUrl(nextSession.baseUrl);
    setError(null);
  }, []);

  const removeServer = useCallback(async (baseUrl: string) => {
    const nextState = await removeSession(baseUrl);
    setSessions(nextState.sessions);
    setSession(nextState.activeSession);
    setRememberedBaseUrl(nextState.activeSession?.baseUrl ?? baseUrl);
    setError(null);
  }, []);

  const reorderServers = useCallback(async (nextSessions: SessionRecord[]) => {
    setSessions(await saveSessionOrder(nextSessions));
  }, []);

  const expireSession = useCallback(
    async (
      expiredSession: SessionRecord,
      message = 'Sign in again to continue.',
    ) => {
      setSessions(await requireSessionLogin(expiredSession));
      setRememberedBaseUrl(expiredSession.baseUrl);
      setSession((current) =>
        current?.baseUrl === expiredSession.baseUrl ? null : current,
      );
      setError(message);
    },
    [],
  );

  const updateDeviceHostname = useCallback(
    async (baseUrl: string, hostname: string) => {
      const trimmedHostname = hostname.trim();
      if (!trimmedHostname) {
        return;
      }

      const storedSessions = await loadSessions();
      const storedSession = storedSessions.find(
        (item) => item.baseUrl === baseUrl,
      );
      if (!storedSession || storedSession.deviceHostname === trimmedHostname) {
        return;
      }

      const nextSession = { ...storedSession, deviceHostname: trimmedHostname };
      setSessions(await saveSession(nextSession));
      setSession((current) =>
        current?.baseUrl === baseUrl ? nextSession : current,
      );
    },
    [],
  );

  const value = useMemo(
    () => ({
      booting,
      clearError,
      error,
      expireSession,
      login,
      rememberedBaseUrl,
      removeServer,
      reorderServers,
      session,
      sessions,
      switchServer,
      updateDeviceHostname,
    }),
    [
      booting,
      clearError,
      error,
      expireSession,
      login,
      rememberedBaseUrl,
      removeServer,
      reorderServers,
      session,
      sessions,
      switchServer,
      updateDeviceHostname,
    ],
  );

  return (
    <SessionContext.Provider value={value}>{children}</SessionContext.Provider>
  );
}

export function useSession(): SessionContextValue {
  const value = useContext(SessionContext);
  if (!value) {
    throw new Error('SessionContext is missing.');
  }
  return value;
}
