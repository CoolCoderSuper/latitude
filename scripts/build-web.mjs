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

export function buildWebAssets({ write = true } = {}) {
  return Promise.all([
    build({
      ...sharedOptions,
      entryPoints: [resolve(assetDirectory, 'file-viewer.js')],
      outfile: resolve(assetDirectory, 'file-viewer.bundle.js'),
      write,
    }),
    build({
      ...sharedOptions,
      entryPoints: [resolve(assetDirectory, 'terminal-viewer.js')],
      outfile: resolve(assetDirectory, 'terminal-viewer.bundle.js'),
      write,
    }),
  ]);
}

if (resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await buildWebAssets();
}
