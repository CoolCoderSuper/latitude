import { build } from 'esbuild';
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const assetDirectory = resolve(repositoryRoot, 'src/server/assets');

async function vendorHtmx(write) {
  const path = resolve(assetDirectory, 'htmx.min.js');
  const contents = await readFile(
    resolve(repositoryRoot, 'node_modules/htmx.org/dist/htmx.min.js'),
  );
  if (write) await writeFile(path, contents);
  return { outputFiles: [{ path, contents }] };
}

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
    vendorHtmx(write),
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
