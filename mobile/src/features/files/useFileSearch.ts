import { useEffect, useRef, useState } from 'react';

import {
  LatitudeRequestCancelledError,
  type LatitudePublicApi,
} from '../../api';
import { LatestRequestManager } from '../../core/async/latestRequest';
import type {
  ProjectFileSearchKind,
  ProjectFileSearchResult,
} from '../../types';
import { errorMessage } from '../../utils/errors';

const SEARCH_DELAY_MS = 180;

export function useFileSearch({
  api,
  projectName,
  query,
  searchKind,
}: {
  api: LatitudePublicApi;
  projectName: string;
  query: string;
  searchKind: ProjectFileSearchKind;
}) {
  const [results, setResults] = useState<ProjectFileSearchResult[]>([]);
  const [limited, setLimited] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestsRef = useRef(new LatestRequestManager());

  useEffect(() => {
    const requests = requestsRef.current;
    const search = query.trim();
    requests.cancel();
    setError(null);

    if (!search) {
      setResults([]);
      setLimited(false);
      setLoading(false);
      return;
    }

    setLoading(true);
    const timer = setTimeout(() => {
      const request = requests.begin(`${projectName}:${searchKind}:${search}`);
      if (!request) return;

      void api
        .searchFiles(projectName, search, searchKind, request.controller.signal)
        .then((response) => {
          if (!requests.isCurrent(request)) return;
          setResults(response.results ?? []);
          setLimited(Boolean(response.limited));
        })
        .catch((searchError) => {
          if (
            requests.isCurrent(request) &&
            !(searchError instanceof LatitudeRequestCancelledError)
          ) {
            setResults([]);
            setLimited(false);
            setError(errorMessage(searchError));
          }
        })
        .finally(() => {
          if (requests.finish(request)) setLoading(false);
        });
    }, SEARCH_DELAY_MS);

    return () => {
      clearTimeout(timer);
      requests.cancel();
    };
  }, [api, projectName, query, searchKind]);

  return { error, limited, loading, results };
}
