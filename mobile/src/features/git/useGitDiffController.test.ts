import { gitActionKey } from './useGitDiffController';

describe('gitActionKey', () => {
  it('deduplicates repository actions by action name', () => {
    expect(gitActionKey({ action: 'push' })).toBe('push');
  });

  it('allows independent file actions to be tracked separately', () => {
    expect(gitActionKey({ action: 'stage_file', path: 'src/app.ts' })).toBe(
      'stage_file:src/app.ts',
    );
  });
});
