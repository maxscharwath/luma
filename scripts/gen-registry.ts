#!/usr/bin/env bun

// Build a static module registry from the packed `.lmod` files: a `catalog.json`
// index (every module's id/name/version/description/icon + file + size + sha256)
// plus the `.lmod` files themselves, ready to publish (GitHub Pages, any static
// host). The in-app Store fetches `catalog.json` and installs a chosen module by
// URL. Mirrors packages/synology-repo (the .spk package source).
//
//   bun run scripts/gen-registry.ts            # from dist/modules/*.lmod
//   bun run scripts/gen-registry.ts --base https://mods.example.com
//
// Output: dist/registry/{catalog.json, <id>.lmod, ...}

import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { $ } from 'bun';

const root = join(import.meta.dir, '..');
const modulesDir = join(root, 'dist/modules');
const outDir = join(root, 'dist/registry');

const baseIdx = process.argv.indexOf('--base');
const baseUrl = baseIdx >= 0 ? process.argv[baseIdx + 1].replace(/\/$/, '') : '';

if (!existsSync(modulesDir)) {
  throw new Error(`no packed modules at ${modulesDir} — run \`bun run modules:pack\` first`);
}
mkdirSync(outDir, { recursive: true });

interface Entry {
  id: string;
  name: string;
  version: string;
  description?: string;
  file: string;
  url: string;
  size: number;
  sha256: string;
}

const lmods = readdirSync(modulesDir).filter((f) => f.endsWith('.lmod'));
const modules: Entry[] = [];

for (const file of lmods) {
  const path = join(modulesDir, file);
  const bytes = readFileSync(path);
  // Read module.json straight out of the archive.
  const manifestJson = await $`tar -xzO -f ${path} module.json`.text();
  const manifest = JSON.parse(manifestJson) as {
    id: string;
    name: string;
    version: string;
    description?: string;
  };
  copyFileSync(path, join(outDir, file));
  modules.push({
    id: manifest.id,
    name: manifest.name,
    version: manifest.version,
    description: manifest.description,
    file,
    url: baseUrl ? `${baseUrl}/${file}` : file,
    size: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
  });
}

modules.sort((a, b) => a.id.localeCompare(b.id));
const catalog = { schema: 1, modules };
writeFileSync(join(outDir, 'catalog.json'), `${JSON.stringify(catalog, null, 2)}\n`);

console.log(`registry: ${modules.length} module(s) -> ${outDir}/catalog.json`);
for (const m of modules) console.log(`  ${m.id}  v${m.version}  (${(m.size / 1024) | 0} KB)`);
