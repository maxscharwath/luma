#!/usr/bin/env bun
// Build a runtime-installable module bundle: compile the WASM backend, build the
// Module Federation frontend, and assemble a `.tar` the admin Store uploads.
//
//   bun run modules:wasm [wasm-modules/<dir>]   (default: wasm-modules/hello-wasm)
//
// Output: dist/wasm-modules/<id>.tar  (module.json + module.wasm + icon.svg + fe/)

import { $ } from 'bun';
import { cpSync, copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dir, '..');
const moduleDir = join(root, process.argv[2] ?? 'wasm-modules/hello-wasm');
const manifest = JSON.parse(readFileSync(join(moduleDir, 'module.json'), 'utf8')) as { id: string };
const { id } = manifest;
console.log(`building module bundle: ${id}`);

// 1) Compile the WASM backend guest.
const beDir = join(moduleDir, 'server');
if (existsSync(beDir)) {
  console.log('  - cargo build --target wasm32-unknown-unknown');
  await $`cargo build --release --target wasm32-unknown-unknown`.cwd(beDir);
}
const wasmOutDir = join(beDir, 'target/wasm32-unknown-unknown/release');
const wasmFile = existsSync(wasmOutDir)
  ? readdirSync(wasmOutDir).find((f) => f.endsWith('.wasm'))
  : undefined;

// 2) Build the Module Federation frontend remote.
const feDir = join(moduleDir, 'ui');
const feDist = join(feDir, 'dist');
if (existsSync(feDir)) {
  console.log('  - vite build (frontend remote)');
  await $`bun run build`.cwd(feDir);
}

// 3) Stage the bundle contents.
const staging = join(moduleDir, '.bundle');
rmSync(staging, { recursive: true, force: true });
mkdirSync(staging, { recursive: true });
const entries: string[] = [];
copyFileSync(join(moduleDir, 'module.json'), join(staging, 'module.json'));
entries.push('module.json');
if (wasmFile) {
  copyFileSync(join(wasmOutDir, wasmFile), join(staging, 'module.wasm'));
  entries.push('module.wasm');
}
for (const icon of ['icon.svg', 'icon.png']) {
  if (existsSync(join(moduleDir, icon))) {
    copyFileSync(join(moduleDir, icon), join(staging, icon));
    entries.push(icon);
  }
}
if (existsSync(feDist)) {
  cpSync(feDist, join(staging, 'fe'), { recursive: true });
  entries.push('fe');
}

// 4) Assemble the tar (explicit entries -> no `./` prefix the unpacker skips).
const outDir = join(root, 'dist/wasm-modules');
mkdirSync(outDir, { recursive: true });
const tarPath = join(outDir, `${id}.tar`);
await $`tar -cf ${tarPath} -C ${staging} ${entries}`;
rmSync(staging, { recursive: true, force: true });

console.log(`\nbundle ready: ${tarPath}`);
console.log('install it in the admin Store (Upload bundle), or via:');
console.log(`  curl -H "Authorization: Bearer <token>" --data-binary @${tarPath} <server>/api/admin/store/install`);
