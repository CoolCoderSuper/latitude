import { readFile } from 'node:fs/promises';

import { buildWebAssets } from './build-web.mjs';

const results = await buildWebAssets({ write: false });
const staleOutputs = [];

for (const result of results) {
  for (const output of result.outputFiles ?? []) {
    let checkedIn;
    try {
      checkedIn = await readFile(output.path);
    } catch {
      staleOutputs.push(output.path);
      continue;
    }
    if (!checkedIn.equals(output.contents)) staleOutputs.push(output.path);
  }
}

if (staleOutputs.length > 0) {
  throw new Error(
    `Generated web assets are stale:\n${staleOutputs.map((path) => `- ${path}`).join('\n')}\nRun npm run build.`,
  );
}
