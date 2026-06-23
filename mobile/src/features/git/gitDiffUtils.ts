import type { GitFileChange, GitFileDiff } from '../../types';

export function canStage(change: GitFileChange): boolean {
  return change.index_status === '?' || change.worktree_status !== ' ';
}

export function canUnstage(change: GitFileChange): boolean {
  return (
    change.index_status !== ' ' &&
    change.index_status !== '?' &&
    change.index_status !== '!'
  );
}

export function statusLabel(change: GitFileChange): string {
  return `${change.index_status}${change.worktree_status}`.replace(/ /g, '-');
}

export function visibleDiffsForSection(
  change: GitFileChange,
  section: 'unstaged' | 'staged',
): GitFileDiff[] {
  if (section === 'unstaged') {
    return change.diffs.filter(
      (diff) => diff.label === 'Unstaged' || diff.label === 'Untracked',
    );
  }

  return change.diffs.filter((diff) => diff.label === 'Staged');
}

export function toggleExpanded(
  setExpanded: (update: (current: Set<string>) => Set<string>) => void,
  key: string,
) {
  setExpanded((current) => {
    const next = new Set(current);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    return next;
  });
}
