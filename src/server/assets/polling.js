export function startVisiblePolling(callback, intervalMs, options = {}) {
  const { immediate = false, runOnVisible = false } = options;
  let stopped = false;
  let running = false;

  const run = async () => {
    if (stopped || running || document.hidden) return;
    running = true;
    try {
      await callback();
    } catch {
      // Polling is best-effort; the next interval can recover automatically.
    } finally {
      running = false;
    }
  };

  const interval = window.setInterval(() => void run(), intervalMs);
  const handleVisibilityChange = () => {
    if (runOnVisible && !document.hidden) void run();
  };
  document.addEventListener('visibilitychange', handleVisibilityChange);

  if (immediate) void run();

  return () => {
    stopped = true;
    window.clearInterval(interval);
    document.removeEventListener('visibilitychange', handleVisibilityChange);
  };
}
