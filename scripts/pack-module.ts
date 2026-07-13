#!/usr/bin/env bun

// Pack modules into installable `.lmod` files: a gzip-compressed tar of
// module.json + optional module.wasm + icon + fe/. Install one from Admin ->
// Modules (Install a module), or POST it to /api/admin/store/install (the server
// auto-detects gzip, so `.lmod` and a plain `.tar` both work).
//
//   bun run modules:pack               # pack EVERY runtime-installable module
//   bun run modules:pack <module-dir>  # pack just one
//
// Output: dist/modules/<id>.lmod

import { existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { $ } from 'bun';
import { buildModuleBundle } from './lib/build-module';

const root = join(import.meta.dir, '..');
const outDir = join(root, 'dist/modules');

// An explicit module dir, else every runtime-installable (WASM) module under
// wasm-modules/ (the compiled-in crate modules ship inside the binary, not as
// .lmod files).
const arg = process.argv[2];
const wasmRoot = join(root, 'wasm-modules');
const moduleDirs = arg
  ? [join(root, arg)]
  : readdirSync(wasmRoot, { withFileTypes: true })
      .filter((e) => e.isDirectory() && existsSync(join(wasmRoot, e.name, 'module.json')))
      .map((e) => join(wasmRoot, e.name))
      .sort();

if (moduleDirs.length === 0) {
  throw new Error('no modules to pack (looked under wasm-modules/)');
}
mkdirSync(outDir, { recursive: true });

const packed: string[] = [];
for (const moduleDir of moduleDirs) {
  const { id, staging, entries } = await buildModuleBundle(moduleDir);
  const lmodPath = join(outDir, `${id}.lmod`);
  // `.lmod` == gzip-compressed tar (`tar -z`); explicit entries so no `./` prefix.
  await $`tar -czf ${lmodPath} -C ${staging} ${entries}`;
  rmSync(staging, { recursive: true, force: true });
  console.log(`  packed: ${lmodPath}`);
  packed.push(lmodPath);
}

console.log(`\n${packed.length} module(s) -> ${outDir}`);
console.log('install from Admin -> Modules (Install a module), or via:');
console.log(
  '  curl -H "Authorization: Bearer <token>" --data-binary @<file>.lmod <server>/api/admin/store/install',
);
