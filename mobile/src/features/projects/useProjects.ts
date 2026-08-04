import { useCallback, useEffect, useRef, useState } from 'react';

import { LatitudeApiError, LatitudeRequestCancelledError } from '../../api';
import { DEFAULT_ROOT_TERMINAL } from '../../constants';
import { useLatitudeApi } from '../../core/api/ApiContext';
import { LatestRequestManager } from '../../core/async/latestRequest';
import { useSession } from '../../core/session/SessionContext';
import type {
  ProjectSummary,
  RootDesktopLink,
  RootTerminalLink,
} from '../../types';
import { errorMessage } from '../../utils/errors';

export type RefreshProjects = (
  fetchRemote?: boolean,
  quiet?: boolean,
  autoRefresh?: boolean,
) => Promise<void>;

export function useProjects() {
  const api = useLatitudeApi();
  const { expireSession, session, updateDeviceHostname } = useSession();
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [rootTerminal, setRootTerminal] = useState<RootTerminalLink>(
    DEFAULT_ROOT_TERMINAL,
  );
  const [rootDesktop, setRootDesktop] = useState<RootDesktopLink | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestsRef = useRef(new LatestRequestManager());
  const activeBaseUrlRef = useRef(session?.baseUrl ?? null);
  const sessionBaseUrl = session?.baseUrl ?? null;

  useEffect(() => {
    activeBaseUrlRef.current = sessionBaseUrl;
    requestsRef.current.cancel();
    setProjects([]);
    setRootTerminal(DEFAULT_ROOT_TERMINAL);
    setRootDesktop(null);
    setError(null);
    setLoading(Boolean(sessionBaseUrl));
  }, [sessionBaseUrl]);

  const refresh = useCallback<RefreshProjects>(
    async (fetchRemote = false, quiet = false, autoRefresh = false) => {
      if (!session) {
        return;
      }

      const request = requestsRef.current.begin(session.baseUrl, true);
      if (!request) {
        return;
      }

      if (!quiet) {
        setLoading(true);
        setError(null);
      }

      try {
        const response = await api.projects(
          fetchRemote,
          autoRefresh,
          request.controller.signal,
        );
        if (
          !requestsRef.current.isCurrent(request) ||
          activeBaseUrlRef.current !== session.baseUrl
        ) {
          return;
        }

        setProjects(response.projects);
        setRootTerminal(response.root_terminal ?? DEFAULT_ROOT_TERMINAL);
        setRootDesktop(response.root_desktop ?? null);
        void updateDeviceHostname(session.baseUrl, response.device_hostname);
      } catch (loadError) {
        if (
          loadError instanceof LatitudeRequestCancelledError ||
          !requestsRef.current.isCurrent(request) ||
          activeBaseUrlRef.current !== session.baseUrl
        ) {
          return;
        }

        if (loadError instanceof LatitudeApiError && loadError.status === 401) {
          await expireSession(session);
          return;
        }
        if (!quiet) {
          setError(errorMessage(loadError));
        }
      } finally {
        if (requestsRef.current.finish(request) && !quiet) {
          setLoading(false);
        }
      }
    },
    [api, expireSession, session, updateDeviceHostname],
  );

  useEffect(() => {
    const requests = requestsRef.current;
    if (session) {
      void refresh();
    }
    return () => requests.cancel();
  }, [refresh, session]);

  const setWorktreeArchived = useCallback(
    async (name: string, archived: boolean) => {
      try {
        await api.setWorktreeArchived(name, archived);
        setProjects((current) =>
          current.map((project) =>
            project.name === name && project.worktree
              ? {
                  ...project,
                  worktree: { ...project.worktree, archived },
                }
              : project,
          ),
        );
      } catch (archiveError) {
        setError(errorMessage(archiveError));
      }
    },
    [api],
  );

  return {
    error,
    loading,
    projects,
    refresh,
    rootDesktop,
    rootTerminal,
    setWorktreeArchived,
  };
}
