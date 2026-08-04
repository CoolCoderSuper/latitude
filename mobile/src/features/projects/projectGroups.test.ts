import type { ProjectSummary } from '../../types';
import { groupProjects } from './projectGroups';

function project(
  name: string,
  repository?: string,
  discovered = false,
): ProjectSummary {
  return {
    name,
    href: `/${name}`,
    api_href: `/api/${name}`,
    summary: name,
    deployment_count: 0,
    git_dirty: false,
    git_additions: 0,
    git_deletions: 0,
    git_ahead: 0,
    git_behind: 0,
    worktree: repository
      ? {
          repository,
          path: `C:/${name}`,
          branch: name,
          head: 'abc',
          discovered,
          archived: false,
        }
      : null,
  };
}

describe('groupProjects', () => {
  it('groups worktrees by repository and uses the configured project label', () => {
    const root = project('Latitude', 'repo');
    const worktree = project('feature-worktree', 'repo', true);

    expect(groupProjects([root, worktree], [root, worktree])).toEqual([
      {
        key: 'repo',
        label: 'Latitude',
        grouped: true,
        projects: [root, worktree],
      },
    ]);
  });

  it('keeps unrelated projects independent', () => {
    const alpha = project('alpha');
    const beta = project('beta');
    expect(groupProjects([alpha, beta], [alpha, beta])).toHaveLength(2);
  });
});
