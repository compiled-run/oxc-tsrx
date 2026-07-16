import { spawnSync } from 'node:child_process';
import { readdir } from 'node:fs/promises';
import { resolve } from 'node:path';

const binary = resolve(process.argv[2]);
const corpus = resolve(process.argv[3]);

async function findTsrxFiles(directory) {
  const files = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.name.startsWith('.') || entry.name === 'node_modules') continue;
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await findTsrxFiles(path)));
    else if (entry.isFile() && entry.name.endsWith('.tsrx')) files.push(path);
  }
  return files;
}

const files = (await findTsrxFiles(corpus)).sort();
const result = spawnSync(binary, files, {
  cwd: corpus,
  encoding: 'utf8',
  maxBuffer: 16 * 1024 * 1024,
});
process.stdout.write(result.stdout);
process.stderr.write(result.stderr);
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
