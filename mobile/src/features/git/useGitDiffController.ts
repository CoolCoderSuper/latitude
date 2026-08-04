import { useCallback, useEffect, useRef, useState } from 'react';

import {
  LatitudeRequestCancelledError,
  type LatitudePublicApi,
} from '../../api';
import { useActivePolling } from '../../core/lifecycle/useActivePolling';
import type { GitActionPayload, GitDiffResponse } from '../../types';
import { errorMessage } from '../../utils/errors';
import { canStage, canUnstage, toggleExpanded } from './gitDiffUtils';

export function gitActionKey(payload: GitActionPayload): string {
  return payload.path ? `${payload.action}:${payload.path}` : payload.action;
}

export function useGitDiffController({
  active,
  api,
  projectName,
}: {
  active: boolean;
  api: LatitudePublicApi;
  projectName: string;
}) {
  const [diff, setDiff] = useState<GitDiffResponse | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [selectedStagedPaths, setSelectedStagedPaths] = useState<Set<string>>(
    new Set(),
  );
  const [loading, setLoading] = useState(true);
  const [pendingActionKeys, setPendingActionKeys] = useState<Set<string>>(
    new Set(),
  );
  const [message, setMessage] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [noticeTone, setNoticeTone] = useState<'success' | 'error'>('success');
  const pendingActionKeysRef = useRef<Set<string>>(new Set());
  const actionQueue = useRef<Promise<void>>(Promise.resolve());
  const refreshPending = useRef(false);
  const refreshControllerRef = useRef<AbortController | null>(null);

  const loadDiff = useCallback(
    async (showLoading = true) => {
      if (refreshPending.current) {
        return;
      }
      refreshPending.current = true;
      const controller = new AbortController();
      refreshControllerRef.current = controller;
      if (showLoading) {
        setLoading(true);
        setNotice(null);
      }
      try {
        const response = await api.diff(projectName, controller.signal);
        if (refreshControllerRef.current === controller) {
          setDiff(response);
        }
      } catch (diffError) {
        if (
          refreshControllerRef.current === controller &&
          !(diffError instanceof LatitudeRequestCancelledError) &&
          showLoading
        ) {
          setNotice(errorMessage(diffError));
          setNoticeTone('error');
        }
      } finally {
        if (refreshControllerRef.current === controller) {
          refreshControllerRef.current = null;
          refreshPending.current = false;
          if (showLoading) {
            setLoading(false);
          }
        }
      }
    },
    [api, projectName],
  );

  useEffect(() => {
    void loadDiff();
    return () => {
      refreshControllerRef.current?.abort();
      refreshControllerRef.current = null;
      refreshPending.current = false;
    };
  }, [loadDiff]);

  const fetchRemote = useCallback(async () => {
    if (refreshPending.current || pendingActionKeysRef.current.size > 0) {
      return;
    }
    refreshPending.current = true;
    try {
      const response = await api.runGitAction(projectName, { action: 'fetch' });
      setDiff(response.diff);
    } catch {
      // Background fetch failures should not interrupt local diff refreshes.
    } finally {
      refreshPending.current = false;
    }
  }, [api, projectName]);

  useActivePolling({
    enabled: active,
    onLocalRefresh: () => loadDiff(false),
    onRemoteRefresh: fetchRemote,
  });

  const runAction = useCallback(
    (payload: GitActionPayload, successMessage: string) => {
      const actionKey = gitActionKey(payload);
      if (pendingActionKeysRef.current.has(actionKey)) {
        return Promise.resolve();
      }

      pendingActionKeysRef.current.add(actionKey);
      setPendingActionKeys(new Set(pendingActionKeysRef.current));
      setNotice(null);

      const execute = async () => {
        try {
          const response = await api.runGitAction(projectName, payload);
          setDiff(response.diff);
          setNotice(
            response.ok ? successMessage : (response.error ?? 'Action failed.'),
          );
          setNoticeTone(response.ok ? 'success' : 'error');
          if (response.ok && payload.action === 'commit') {
            setMessage('');
          }
          if (response.ok && payload.action === 'stage_selected') {
            setSelectedPaths(new Set());
          }
          if (response.ok && payload.action === 'unstage_selected') {
            setSelectedStagedPaths(new Set());
          }
        } catch (actionError) {
          setNotice(errorMessage(actionError));
          setNoticeTone('error');
        } finally {
          pendingActionKeysRef.current.delete(actionKey);
          setPendingActionKeys(new Set(pendingActionKeysRef.current));
        }
      };

      const queuedAction = actionQueue.current.then(execute, execute);
      actionQueue.current = queuedAction;
      return queuedAction;
    },
    [api, projectName],
  );

  const unstaged = diff?.file_changes.filter(canStage) ?? [];
  const staged = diff?.file_changes.filter(canUnstage) ?? [];

  useEffect(() => {
    const availablePaths = new Set(
      (diff?.file_changes ?? []).filter(canStage).map((change) => change.path),
    );
    setSelectedPaths((current) => {
      const next = new Set(
        Array.from(current).filter((path) => availablePaths.has(path)),
      );
      return next.size === current.size ? current : next;
    });
    const availableStagedPaths = new Set(
      (diff?.file_changes ?? [])
        .filter(canUnstage)
        .map((change) => change.path),
    );
    setSelectedStagedPaths((current) => {
      const next = new Set(
        Array.from(current).filter((path) => availableStagedPaths.has(path)),
      );
      return next.size === current.size ? current : next;
    });
  }, [diff]);

  const toggleSelected = useCallback((path: string) => {
    setSelectedPaths((current) => toggleSetValue(current, path));
  }, []);

  const toggleStagedSelected = useCallback((path: string) => {
    setSelectedStagedPaths((current) => toggleSetValue(current, path));
  }, []);

  const toggleFileExpanded = useCallback((path: string) => {
    toggleExpanded(setExpanded, path);
  }, []);

  return {
    diff,
    expanded,
    loadDiff,
    loading,
    message,
    notice,
    noticeTone,
    pendingActionKeys,
    runAction,
    selectedPaths,
    selectedStagedPaths,
    setMessage,
    staged,
    toggleFileExpanded,
    toggleSelected,
    toggleStagedSelected,
    unstaged,
  };
}

function toggleSetValue(current: Set<string>, value: string): Set<string> {
  const next = new Set(current);
  if (next.has(value)) {
    next.delete(value);
  } else {
    next.add(value);
  }
  return next;
}
