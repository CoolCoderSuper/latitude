import { useCallback, useEffect, useRef, useState } from 'react';

import {
  LatitudeRequestCancelledError,
  type LatitudePublicApi,
} from '../../api';
import { LatestRequestManager } from '../../core/async/latestRequest';
import { useActivePolling } from '../../core/lifecycle/useActivePolling';
import type { ProjectDetail } from '../../types';
import { errorMessage } from '../../utils/errors';

export type RefreshProject = (
  fetchRemote?: boolean,
  quiet?: boolean,
  autoRefresh?: boolean,
) => Promise<void>;

export function useProject({
  active,
  api,
  fetchRemote,
  projectName,
}: {
  active: boolean;
  api: LatitudePublicApi;
  fetchRemote: boolean;
  projectName: string;
}) {
  const [project, setProject] = useState<ProjectDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestsRef = useRef(new LatestRequestManager());
  const activeProjectRef = useRef(projectName);

  useEffect(() => {
    activeProjectRef.current = projectName;
    requestsRef.current.cancel();
    setProject(null);
    setLoading(true);
    setError(null);
  }, [projectName]);

  const refresh = useCallback<RefreshProject>(
    async (refreshRemote = false, quiet = false, autoRefresh = false) => {
      const request = requestsRef.current.begin(projectName, autoRefresh);
      if (!request) {
        return;
      }
      if (!quiet) {
        setLoading(true);
        setError(null);
      }

      try {
        const response = await api.project(
          projectName,
          refreshRemote,
          autoRefresh,
          request.controller.signal,
        );
        if (
          requestsRef.current.isCurrent(request) &&
          activeProjectRef.current === projectName
        ) {
          setProject(response);
        }
      } catch (projectError) {
        if (
          requestsRef.current.isCurrent(request) &&
          !(projectError instanceof LatitudeRequestCancelledError) &&
          !quiet
        ) {
          setError(errorMessage(projectError));
        }
      } finally {
        if (requestsRef.current.finish(request) && !quiet) {
          setLoading(false);
        }
      }
    },
    [api, projectName],
  );

  useEffect(() => {
    const requests = requestsRef.current;
    void refresh(true);
    return () => requests.cancel();
  }, [refresh]);

  useActivePolling({
    enabled: active,
    onLocalRefresh: () => refresh(false, true, true),
    onRemoteRefresh: () => refresh(fetchRemote, true, true),
  });

  return { error, loading, project, refresh };
}
