#!/usr/bin/env bun

// Pack modules into installable `.lmod` files. A `.lmod` is a gzip-compressed tar
// of a module's native binary (`module`, the out-of-process runtime entrypoint)
// + `module.json` + `icon.<ext>` + its frontend remote (`fe/`, when it ships one).
// Install one from Admin -> Modules, or POST it to /api/admin/store/install; the
// core unpacks it under <data>/modules/<id>/ and the supervisor spawns it.
//
//   bun run modules:pack               # pack every module that has a [[bin]]
//   bun run modules:pack <module-dir>  # pack just one
//
// Output: dist/modules/<id>.lmod  (host target; CI packs one per platform)

import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { $ } from 'bun';

const root = join(import.meta.dir, '..');
const outDir = join(root, 'dist/modules');
const modulesRoot = join(root, 'server/modules');

/** Modules with an out-of-process binary ([[bin]] in their server Cargo.toml). */
function packableModules(): string[] {
  return readdirSync(modulesRoot, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => join(modulesRoot, e.name))
    .filter((dir) => {
      const cargo = join(dir, 'server/Cargo.toml');
      return existsSync(cargo) && readFileSync(cargo, 'utf8').includes('[[bin]]');
    });
}

/**
 * The crate (package) name, its `[[bin]]` name, and any cargo features the
 * `.lmod` build should enable, from a module's server Cargo.toml. Features are
 * declared as `[package.metadata.lmod] features = ["rqbit"]` so the build stays
 * declarative (e.g. torrents bundles the embedded librqbit engine).
 */
function crateAndBin(moduleDir: string): { pkg: string; bin: string; features: string[] } {
  const cargo = readFileSync(join(moduleDir, 'server/Cargo.toml'), 'utf8');
  const pkg = cargo.match(/\[package\][\s\S]*?name\s*=\s*"([^"]+)"/)?.[1];
  const bin = cargo.match(/\[\[bin\]\][\s\S]*?name\s*=\s*"([^"]+)"/)?.[1];
  if (!pkg || !bin) throw new Error(`missing package/[[bin]] name in ${moduleDir}/server/Cargo.toml`);
  const featBlock = cargo.match(/\[package\.metadata\.lmod\][\s\S]*?features\s*=\s*\[([^\]]*)\]/)?.[1] ?? '';
  const features = [...featBlock.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  return { pkg, bin, features };
}

async function packOne(moduleDir: string): Promise<string> {
  const manifest = JSON.parse(readFileSync(join(moduleDir, 'module.json'), 'utf8')) as { id: string };
  const { id } = manifest;
  const { pkg, bin, features } = crateAndBin(moduleDir);
  console.log(`\npacking ${id} (${pkg} -> bin ${bin}${features.length ? ` [+${features.join(',')}]` : ''})`);

  // 1) Build the module's native binary (release), with any declared features.
  const featArgs = features.length ? ['--features', features.join(',')] : [];
  await $`cargo build --release -p ${pkg} --bin ${bin} ${featArgs}`.cwd(join(root, 'server'));
  const binPath = join(root, 'server/target/release', bin);
  if (!existsSync(binPath)) throw new Error(`built no binary at ${binPath}`);

  // 2) Build the frontend remote if the module ships one.
  const uiDir = join(moduleDir, 'ui');
  const feDist = join(uiDir, 'dist');
  if (existsSync(uiDir) && existsSync(join(uiDir, 'vite.config.ts'))) {
    console.log('  - vite build (frontend remote)');
    await $`bun run build`.cwd(uiDir).nothrow();
  }

  // 3) Stage the bundle.
  const staging = join(moduleDir, '.bundle');
  rmSync(staging, { recursive: true, force: true });
  mkdirSync(staging, { recursive: true });
  const entries: string[] = [];
  copyFileSync(join(moduleDir, 'module.json'), join(staging, 'module.json'));
  entries.push('module.json');
  copyFileSync(binPath, join(staging, 'module'));
  entries.push('module');
  for (const icon of ['icon.svg', 'icon.png']) {
    if (existsSync(join(moduleDir, icon))) {
      copyFileSync(join(moduleDir, icon), join(staging, icon));
      entries.push(icon);
    }
  }
  if (existsSync(feDist)) {
    await $`cp -R ${feDist} ${join(staging, 'fe')}`;
    entries.push('fe');
  }

  // 4) gzip-tar it into a .lmod.
  mkdirSync(outDir, { recursive: true });
  const lmod = join(outDir, `${id}.lmod`);
  await $`tar -czf ${lmod} -C ${staging} ${entries}`;
  rmSync(staging, { recursive: true, force: true });
  console.log(`  packed: ${lmod}`);
  return lmod;
}

const arg = process.argv[2];
const dirs = arg ? [join(root, arg)] : packableModules();
if (dirs.length === 0) {
  throw new Error('no packable modules (none have a [[bin]] yet)');
}
const packed: string[] = [];
for (const dir of dirs) {
  packed.push(await packOne(dir));
}
console.log(`\n${packed.length} module(s) -> ${outDir}`);
console.log('install from Admin -> Modules, or POST to /api/admin/store/install.');
