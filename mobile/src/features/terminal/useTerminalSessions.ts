import { useCallback, useEffect, useState } from 'react';

import type { TerminalSessionSummary } from '../../types';
import { errorMessage } from '../../utils/errors';

export type TerminalTarget = {
  title: string;
  terminalHref: string;
  listSessions: () => Promise<{ sessions: TerminalSessionSummary[] }>;
  createSession: () => Promise<TerminalSessionSummary>;
  closeSession: (sessionId: string) => Promise<void>;
};

export function useTerminalSessions(target: TerminalTarget) {
  const [sessions, setSessions] = useState<TerminalSessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [closingSessionId, setClosingSessionId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadSessions = useCallback(async () => {
    setLoading(true);
    setNotice(null);
    try {
      let nextSessions = (await target.listSessions()).sessions;
      if (nextSessions.length === 0) {
        nextSessions = [await target.createSession()];
      }
      setSessions(nextSessions);
      setActiveSessionId((current) =>
        nextSessions.some((item) => item.id === current)
          ? current
          : (nextSessions[0]?.id ?? null),
      );
    } catch (sessionError) {
      setNotice(errorMessage(sessionError));
    } finally {
      setLoading(false);
    }
  }, [target]);

  useEffect(() => {
    void loadSessions();
  }, [loadSessions]);

  const createSession = useCallback(async () => {
    if (creating) {
      return;
    }
    setCreating(true);
    setNotice(null);
    try {
      const created = await target.createSession();
      setSessions((current) => [...current, created]);
      setActiveSessionId(created.id);
    } catch (sessionError) {
      setNotice(errorMessage(sessionError));
    } finally {
      setCreating(false);
    }
  }, [creating, target]);

  const closeSession = useCallback(
    async (sessionId: string) => {
      if (closingSessionId) {
        return;
      }
      setClosingSessionId(sessionId);
      setNotice(null);
      try {
        await target.closeSession(sessionId);
        setSessions((current) => {
          const next = current.filter((item) => item.id !== sessionId);
          setActiveSessionId((active) =>
            active === sessionId ? (next[0]?.id ?? null) : active,
          );
          return next;
        });
      } catch (sessionError) {
        setNotice(errorMessage(sessionError));
      } finally {
        setClosingSessionId(null);
      }
    },
    [closingSessionId, target],
  );

  return {
    activeSessionId,
    closeSession,
    closingSessionId,
    createSession,
    creating,
    loading,
    notice,
    sessions,
    setActiveSessionId,
  };
}
