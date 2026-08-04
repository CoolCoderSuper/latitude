import { useEffect, useRef } from 'react';
import { AppState } from 'react-native';

export function useActivePolling({
  enabled,
  onLocalRefresh,
  onRemoteRefresh,
  refreshIntervalMs = 2_000,
  remoteIntervalMs = 30_000,
}: {
  enabled: boolean;
  onLocalRefresh: () => void | Promise<void>;
  onRemoteRefresh?: () => void | Promise<void>;
  refreshIntervalMs?: number;
  remoteIntervalMs?: number;
}) {
  const localRefreshRef = useRef(onLocalRefresh);
  const remoteRefreshRef = useRef(onRemoteRefresh);

  useEffect(() => {
    localRefreshRef.current = onLocalRefresh;
    remoteRefreshRef.current = onRemoteRefresh;
  }, [onLocalRefresh, onRemoteRefresh]);

  useEffect(() => {
    if (!enabled) {
      return;
    }

    let appActive = AppState.currentState === 'active';
    const refreshLocal = () => {
      if (appActive) {
        void localRefreshRef.current();
      }
    };
    const refreshRemote = () => {
      if (appActive) {
        void (remoteRefreshRef.current ?? localRefreshRef.current)();
      }
    };

    refreshRemote();
    const refreshInterval = setInterval(refreshLocal, refreshIntervalMs);
    const remoteInterval = setInterval(refreshRemote, remoteIntervalMs);
    const subscription = AppState.addEventListener('change', (state) => {
      const wasActive = appActive;
      appActive = state === 'active';
      if (appActive && !wasActive) {
        refreshRemote();
      }
    });

    return () => {
      clearInterval(refreshInterval);
      clearInterval(remoteInterval);
      subscription.remove();
    };
  }, [enabled, refreshIntervalMs, remoteIntervalMs]);
}
