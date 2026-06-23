import type { AppStyles } from '../../theme';
import type { DiffLine, DiffLineKind, DiffTokenKind } from '../../types';

export function diffLineStyle(kind: DiffLineKind | undefined, styles: AppStyles) {
  switch (kind) {
    case 'file':
      return styles.diffLineFile;
    case 'hunk':
      return styles.diffLineHunk;
    case 'add':
      return styles.diffLineAdd;
    case 'remove':
      return styles.diffLineRemove;
    default:
      return undefined;
  }
}

export function tokenStyle(kind: DiffTokenKind | undefined, styles: AppStyles) {
  switch (kind) {
    case 'comment':
      return styles.tokenComment;
    case 'keyword':
      return styles.tokenKeyword;
    case 'number':
      return styles.tokenNumber;
    case 'property':
      return styles.tokenProperty;
    case 'punctuation':
      return styles.tokenPunctuation;
    case 'string':
      return styles.tokenString;
    case 'type':
      return styles.tokenType;
    default:
      return undefined;
  }
}

export function fallbackDiffLines(content: string): DiffLine[] {
  if (!content.length) {
    return [];
  }

  return content.split(/\r?\n/).map((line) => ({
    kind: diffLineKind(line),
    tokens: [{ text: line || ' ' }],
  }));
}

function diffLineKind(line: string): DiffLineKind | undefined {
  if (
    line.startsWith('diff --git') ||
    line.startsWith('index ') ||
    line.startsWith('new file mode ') ||
    line.startsWith('deleted file mode ') ||
    line.startsWith('--- ') ||
    line.startsWith('+++ ')
  ) {
    return 'file';
  }
  if (line.startsWith('@@')) {
    return 'hunk';
  }
  if (line.startsWith('+')) {
    return 'add';
  }
  if (line.startsWith('-')) {
    return 'remove';
  }

  return undefined;
}
