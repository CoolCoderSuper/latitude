import { build } from 'esbuild';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const assetDirectory = resolve(repositoryRoot, 'src/server/assets');

const sharedOptions = {
  absWorkingDir: repositoryRoot,
  bundle: true,
  format: 'esm',
  legalComments: 'eof',
  minify: true,
  platform: 'browser',
  target: 'es2022',
};

// Classic scripts run synchronously in the head; page scripts stay ES modules.
const classicEntries = [
  'htmx',
  'theme-bootstrap',
  'theme-toggle',
  'editor-preference',
];
const moduleEntries = [
  'project-home',
  'diff-viewer',
  'git-history',
  'desktop-viewer',
  'file-viewer',
  'terminal-viewer',
  'neovim',
];

export function buildWebAssets({ write = true } = {}) {
  return Promise.all(
    [...classicEntries, ...moduleEntries].map((name) =>
      build({
        ...sharedOptions,
        format: classicEntries.includes(name) ? 'iife' : 'esm',
        entryPoints: [resolve(assetDirectory, `${name}.js`)],
        outfile: resolve(assetDirectory, `${name}.bundle.js`),
        write,
      }),
    ),
  );
}

if (resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await buildWebAssets();
}
