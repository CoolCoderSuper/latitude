import type { AppStyles } from '../../theme';

export type SyntaxLanguage = 'plain' | 'rust' | 'javascript' | 'css' | 'html' | 'json' | 'config';
export type TokenKind =
  | 'comment'
  | 'keyword'
  | 'number'
  | 'property'
  | 'punctuation'
  | 'string'
  | 'type';
export type DiffToken = {
  text: string;
  kind?: TokenKind;
};

export function diffLineStyle(line: string, styles: AppStyles) {
  if (
    line.startsWith('diff --git') ||
    line.startsWith('index ') ||
    line.startsWith('new file mode ') ||
    line.startsWith('deleted file mode ') ||
    line.startsWith('--- ') ||
    line.startsWith('+++ ')
  ) {
    return styles.diffLineFile;
  }
  if (line.startsWith('@@')) {
    return styles.diffLineHunk;
  }
  if (line.startsWith('+')) {
    return styles.diffLineAdd;
  }
  if (line.startsWith('-')) {
    return styles.diffLineRemove;
  }

  return undefined;
}

export function tokenStyle(kind: TokenKind | undefined, styles: AppStyles) {
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

export function renderDiffLineTokens(line: string, language: SyntaxLanguage): DiffToken[] {
  if (isDiffHeaderLine(line) || line.startsWith('@@')) {
    return [{ text: line }];
  }

  const first = line[0];
  if (first === '+' || first === '-' || first === ' ') {
    return [{ text: first }, ...syntaxTokens(line.slice(1), language)];
  }

  return syntaxTokens(line, language);
}

function isDiffHeaderLine(line: string): boolean {
  return (
    line.startsWith('diff --git') ||
    line.startsWith('index ') ||
    line.startsWith('new file mode ') ||
    line.startsWith('deleted file mode ') ||
    line.startsWith('--- ') ||
    line.startsWith('+++ ')
  );
}

export function syntaxLanguageForPath(path: string): SyntaxLanguage {
  const lower = path.toLowerCase();
  const name = lower.split(/[\\/]/).pop() ?? lower;

  if (
    [
      'cargo.toml',
      'cargo.lock',
      'package.json',
      'tsconfig.json',
      'vite.config.js',
      'vite.config.ts',
      'svelte.config.js',
      'svelte.config.ts',
    ].includes(name)
  ) {
    if (name.endsWith('.json')) {
      return 'json';
    }
    if (name.endsWith('.js') || name.endsWith('.ts')) {
      return 'javascript';
    }
    return 'config';
  }

  const extension = name.split('.').pop() ?? '';
  switch (extension) {
    case 'rs':
      return 'rust';
    case 'js':
    case 'jsx':
    case 'mjs':
    case 'cjs':
    case 'ts':
    case 'tsx':
    case 'svelte':
      return 'javascript';
    case 'css':
    case 'scss':
    case 'sass':
      return 'css';
    case 'html':
    case 'htm':
    case 'xml':
    case 'svg':
      return 'html';
    case 'json':
      return 'json';
    case 'toml':
    case 'yaml':
    case 'yml':
    case 'env':
    case 'ini':
    case 'conf':
    case 'lock':
      return 'config';
    default:
      return 'plain';
  }
}

function syntaxTokens(line: string, language: SyntaxLanguage): DiffToken[] {
  if (language === 'plain') {
    return [{ text: line }];
  }

  const tokens: DiffToken[] = [];
  let index = 0;

  while (index < line.length) {
    const rest = line.slice(index);
    const commentLength = commentTokenLength(rest, language);
    if (commentLength) {
      tokens.push({ text: rest.slice(0, commentLength), kind: 'comment' });
      index += commentLength;
      continue;
    }

    const ch = rest[0];
    if (ch === '"' || ch === "'" || ch === '`') {
      const length = stringTokenLength(rest, ch);
      const kind =
        language === 'json' && followedByColon(rest.slice(length))
          ? 'property'
          : 'string';
      tokens.push({ text: rest.slice(0, length), kind });
      index += length;
      continue;
    }

    if (language === 'css' && ch === '#') {
      const length = cssColorTokenLength(rest);
      if (length > 1) {
        tokens.push({ text: rest.slice(0, length), kind: 'number' });
        index += length;
        continue;
      }
    }

    if (isAsciiDigit(ch)) {
      const length = numberTokenLength(rest);
      tokens.push({ text: rest.slice(0, length), kind: 'number' });
      index += length;
      continue;
    }

    if (isIdentifierStart(ch)) {
      const length = identifierTokenLength(rest);
      const text = rest.slice(0, length);
      tokens.push({
        text,
        kind: identifierTokenKind(language, text, rest.slice(length)),
      });
      index += length;
      continue;
    }

    if (isPunctuation(ch)) {
      tokens.push({ text: ch, kind: 'punctuation' });
      index += 1;
      continue;
    }

    tokens.push({ text: ch });
    index += 1;
  }

  return tokens;
}

function commentTokenLength(rest: string, language: SyntaxLanguage) {
  if ((language === 'rust' || language === 'javascript') && rest.startsWith('//')) {
    return rest.length;
  }
  if (language === 'css' && rest.startsWith('/*')) {
    const end = rest.indexOf('*/');
    return end === -1 ? rest.length : end + 2;
  }
  if (language === 'html' && rest.startsWith('<!--')) {
    const end = rest.indexOf('-->');
    return end === -1 ? rest.length : end + 3;
  }
  if (language === 'config' && rest.startsWith('#')) {
    return rest.length;
  }

  return 0;
}

function stringTokenLength(rest: string, quote: string) {
  let escaped = false;
  for (let index = 1; index < rest.length; index += 1) {
    const ch = rest[index];
    if (escaped) {
      escaped = false;
    } else if (ch === '\\') {
      escaped = true;
    } else if (ch === quote) {
      return index + 1;
    }
  }

  return rest.length;
}

function cssColorTokenLength(rest: string) {
  let length = 1;
  while (length < rest.length && /[0-9a-f]/i.test(rest[length])) {
    length += 1;
  }
  return length;
}

function numberTokenLength(rest: string) {
  let length = 0;
  while (length < rest.length && /[0-9a-z_.]/i.test(rest[length])) {
    length += 1;
  }
  return length;
}

function identifierTokenLength(rest: string) {
  let length = 0;
  while (length < rest.length && isIdentifierContinue(rest[length])) {
    length += 1;
  }
  return length;
}

function isIdentifierStart(ch: string) {
  return ch === '_' || /[a-z]/i.test(ch);
}

function isIdentifierContinue(ch: string) {
  return ch === '_' || ch === '-' || /[0-9a-z]/i.test(ch);
}

function isAsciiDigit(ch: string) {
  return /[0-9]/.test(ch);
}

function isPunctuation(ch: string) {
  return '{}[]()<>;:,.=+-*/!?|&%'.includes(ch);
}

function followedByColon(rest: string) {
  return rest.trimStart().startsWith(':');
}

function identifierTokenKind(
  language: SyntaxLanguage,
  token: string,
  following: string,
): TokenKind | undefined {
  if (isKeyword(language, token)) {
    return 'keyword';
  }
  if (isTypeToken(language, token)) {
    return 'type';
  }
  if (language === 'css' && followedByColon(following)) {
    return 'property';
  }
  return undefined;
}

function isKeyword(language: SyntaxLanguage, token: string) {
  switch (language) {
    case 'rust':
      return [
        'as',
        'async',
        'await',
        'break',
        'const',
        'continue',
        'crate',
        'else',
        'enum',
        'extern',
        'false',
        'fn',
        'for',
        'if',
        'impl',
        'in',
        'let',
        'loop',
        'match',
        'mod',
        'move',
        'mut',
        'pub',
        'ref',
        'return',
        'self',
        'Self',
        'static',
        'struct',
        'super',
        'trait',
        'true',
        'type',
        'unsafe',
        'use',
        'where',
        'while',
      ].includes(token);
    case 'javascript':
      return [
        'as',
        'async',
        'await',
        'break',
        'case',
        'catch',
        'class',
        'const',
        'continue',
        'default',
        'else',
        'export',
        'extends',
        'false',
        'finally',
        'for',
        'from',
        'function',
        'if',
        'import',
        'in',
        'interface',
        'let',
        'new',
        'null',
        'return',
        'switch',
        'this',
        'throw',
        'true',
        'try',
        'type',
        'typeof',
        'var',
        'while',
      ].includes(token);
    case 'css':
      return ['and', 'from', 'important', 'keyframes', 'media', 'not', 'only', 'supports', 'to'].includes(token);
    case 'json':
      return ['false', 'null', 'true'].includes(token);
    case 'html':
      return token === 'DOCTYPE';
    default:
      return false;
  }
}

function isTypeToken(language: SyntaxLanguage, token: string) {
  if (language === 'rust') {
    return (
      [
        'bool',
        'char',
        'f32',
        'f64',
        'i8',
        'i16',
        'i32',
        'i64',
        'i128',
        'isize',
        'str',
        'String',
        'u8',
        'u16',
        'u32',
        'u64',
        'u128',
        'usize',
      ].includes(token) || /^[A-Z]/.test(token)
    );
  }

  return (language === 'javascript' || language === 'html') && /^[A-Z]/.test(token);
}
