import { useCallback, useEffect, useRef, useState } from 'react';

import {
  LatitudeRequestCancelledError,
  type LatitudePublicApi,
} from '../../api';
import { LatestRequestManager } from '../../core/async/latestRequest';
import type { ProjectFileEntry } from '../../types';
import { errorMessage } from '../../utils/errors';

export function useFileBrowser({
  api,
  projectName,
}: {
  api: LatitudePublicApi;
  projectName: string;
}) {
  const [path, setPath] = useState('');
  const [entries, setEntries] = useState<ProjectFileEntry[]>([]);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [selectedLine, setSelectedLine] = useState<number | null>(null);
  const [selectedColumn, setSelectedColumn] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const requestsRef = useRef(new LatestRequestManager());

  const loadFolder = useCallback(
    async (nextPath: string) => {
      const request = requestsRef.current.begin(`${projectName}:${nextPath}`);
      if (!request) {
        return;
      }
      setLoading(true);
      setError(null);
      try {
        const response = await api.files(
          projectName,
          nextPath,
          request.controller.signal,
        );
        if (requestsRef.current.isCurrent(request)) {
          setPath(response.path);
          setEntries(response.entries);
        }
      } catch (loadError) {
        if (
          requestsRef.current.isCurrent(request) &&
          !(loadError instanceof LatitudeRequestCancelledError)
        ) {
          setError(errorMessage(loadError));
        }
      } finally {
        if (requestsRef.current.finish(request)) {
          setLoading(false);
        }
      }
    },
    [api, projectName],
  );

  useEffect(() => {
    const requests = requestsRef.current;
    setSelectedFile(null);
    setSelectedLine(null);
    setSelectedColumn(null);
    void loadFolder('');
    return () => requests.cancel();
  }, [loadFolder]);

  const goBack = useCallback(() => {
    if (selectedFile) {
      setSelectedFile(null);
      setSelectedLine(null);
      setSelectedColumn(null);
      return;
    }
    if (!path) {
      return;
    }
    const parts = path.split('/').filter(Boolean);
    parts.pop();
    void loadFolder(parts.join('/'));
  }, [loadFolder, path, selectedFile]);

  const openFile = useCallback(
    (
      filePath: string,
      line: number | null = null,
      column: number | null = null,
    ) => {
      const parts = filePath.split('/').filter(Boolean);
      parts.pop();
      setSelectedFile(filePath);
      setSelectedLine(line);
      setSelectedColumn(column);
      void loadFolder(parts.join('/'));
    },
    [loadFolder],
  );

  const selectEntry = useCallback(
    (entry: ProjectFileEntry) => {
      if (entry.kind === 'directory') {
        void loadFolder(entry.path);
      } else {
        openFile(entry.path);
      }
    },
    [loadFolder, openFile],
  );

  return {
    canGoBack: Boolean(selectedFile || path),
    entries,
    error,
    goBack,
    loading,
    openFile,
    path,
    selectEntry,
    selectedFile,
    selectedLine,
    selectedColumn,
  };
}
