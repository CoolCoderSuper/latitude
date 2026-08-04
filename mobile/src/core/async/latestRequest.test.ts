import { LatestRequestManager } from './latestRequest';

describe('LatestRequestManager', () => {
  it('cancels an obsolete request and keeps only the latest request current', () => {
    const manager = new LatestRequestManager();
    const first = manager.begin('first');
    const second = manager.begin('second');

    expect(first).not.toBeNull();
    expect(second).not.toBeNull();
    expect(first?.controller.signal.aborted).toBe(true);
    expect(first && manager.isCurrent(first)).toBe(false);
    expect(second && manager.isCurrent(second)).toBe(true);
  });

  it('deduplicates a request with the same active key', () => {
    const manager = new LatestRequestManager();
    const first = manager.begin('projects', true);

    expect(first).not.toBeNull();
    expect(manager.begin('projects', true)).toBeNull();
    expect(first && manager.finish(first)).toBe(true);
    expect(manager.begin('projects', true)).not.toBeNull();
  });
});
