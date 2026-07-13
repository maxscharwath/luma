#!/usr/bin/env bun

// Build a runtime-installable module bundle as a raw `.tar`. Prefer
// `bun run modules:pack` (a gzip-compressed `.lmod`); this stays for the plain
// `.tar` output the install endpoint has always accepted.
//
//   bun run modules:wasm [<module-dir>]     (default: wasm-modules/dev.luma.hellowasm)
//
// Output: dist/wasm-modules/<id>.tar  (module.json + module.wasm + icon + fe/)

import { mkdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { $ } from 'bun';
import { buildModuleBundle } from './lib/build-module';

const root = join(import.meta.dir, '..');
const moduleDir = join(root, process.argv[2] ?? 'wasm-modules/dev.luma.hellowasm');

const { id, staging, entries } = await buildModuleBundle(moduleDir);

const outDir = join(root, 'dist/wasm-modules');
mkdirSync(outDir, { recursive: true });
const tarPath = join(outDir, `${id}.tar`);
await $`tar -cf ${tarPath} -C ${staging} ${entries}`;
rmSync(staging, { recursive: true, force: true });

console.log(`\nbundle ready: ${tarPath}`);
console.log('install it from Admin -> Modules (Install a module), or via:');
console.log(
  `  curl -H "Authorization: Bearer <token>" --data-binary @${tarPath} <server>/api/admin/store/install`,
);
