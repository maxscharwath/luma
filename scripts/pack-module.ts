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

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { $ } from 'bun';

const root = join(import.meta.dir, '..');
const outDir = join(root, 'dist/modules');
const modulesRoot = join(root, 'server/modules');

/** Every module (any dir with a module.json). Modules with a `[[bin]]` pack a
 * native sidecar; those without pack as a "library" module (manifest + FE only,
 * no spawned process — e.g. the release-name parser, whose code is co-linked). */
function packableModules(): string[] {
  return readdirSync(modulesRoot, { withFileTypes: true })
    .filter((e) => e.isDirectory())
    .map((e) => join(modulesRoot, e.name))
    .filter((dir) => existsSync(join(dir, 'module.json')));
}

/**
 * The crate (package) name, its `[[bin]]` name, and any cargo features the
 * `.lmod` build should enable, from a module's server Cargo.toml. Features are
 * declared as `[package.metadata.lmod] features = ["rqbit"]` so the build stays
 * declarative (e.g. torrents bundles the embedded librqbit engine).
 */
function crateAndBin(moduleDir: string): { pkg: string; bin: string | null; features: string[] } {
  const cargoPath = join(moduleDir, 'server/Cargo.toml');
  // A module may have no server crate at all (pure FE); then it's library-only.
  const cargo = existsSync(cargoPath) ? readFileSync(cargoPath, 'utf8') : '';
  const pkg = cargo.match(/\[package\][\s\S]*?name\s*=\s*"([^"]+)"/)?.[1] ?? '';
  // A `[[bin]]` means a native sidecar; its absence => a library module (no binary).
  const bin = cargo.match(/\[\[bin\]\][\s\S]*?name\s*=\s*"([^"]+)"/)?.[1] ?? null;
  const featBlock = cargo.match(/\[package\.metadata\.lmod\][\s\S]*?features\s*=\s*\[([^\]]*)\]/)?.[1] ?? '';
  const features = [...featBlock.matchAll(/"([^"]+)"/g)].map((m) => m[1]);
  return { pkg, bin, features };
}

async function packOne(moduleDir: string): Promise<string> {
  const manifest = JSON.parse(readFileSync(join(moduleDir, 'module.json'), 'utf8')) as { id: string };
  const { id } = manifest;
  const { pkg, bin, features } = crateAndBin(moduleDir);
  const kind = bin ? `${pkg} -> bin ${bin}${features.length ? ` [+${features.join(',')}]` : ''}` : 'library (no binary)';
  console.log(`\npacking ${id} (${kind})`);

  // 1) Build the module's native binary, with any declared features. Uses the
  //    `release-lmod` profile (release + panic=abort): a sidecar aborts on panic
  //    and the supervisor respawns it, which drops the unwinding tables for ~11%
  //    smaller binaries at no speed cost (opt-level stays 3). Library modules (no
  //    [[bin]]) skip this: their code is co-linked, not spawned.
  // Optional cross-target (LMOD_TARGET, e.g. x86_64-unknown-linux-musl). A .lmod
  // carries a NATIVE binary, so the platform must match the server; when set, the
  // bundle is suffixed with the triple (see the output name below). Unset = host.
  const target = process.env.LMOD_TARGET?.trim() || null;
  let binPath: string | null = null;
  if (bin) {
    const featArgs = features.length ? ['--features', features.join(',')] : [];
    const targetArgs = target ? ['--target', target] : [];
    await $`cargo build --profile release-lmod -p ${pkg} --bin ${bin} ${featArgs} ${targetArgs}`.cwd(
      join(root, 'server'),
    );
    // cargo nests the artifact under target/<triple>/ when --target is given.
    const outRoot = target ? `server/target/${target}/release-lmod` : 'server/target/release-lmod';
    binPath = join(root, outRoot, bin);
    if (!existsSync(binPath)) throw new Error(`built no binary at ${binPath}`);
  }

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
  if (binPath) {
    copyFileSync(binPath, join(staging, 'module'));
    entries.push('module');
  }
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

  // 4) tar + zstd it into a .lmod (zstd is ~20-25% smaller than gzip for these
  //    native binaries; the supervisor decompresses it with pure-Rust ruzstd).
  //    Sidecar bundles are suffixed with the build target so a consumer can tell
  //    which platform they run on; library modules (no binary) stay unsuffixed.
  mkdirSync(outDir, { recursive: true });
  const suffix = bin && target ? `-${target}` : '';
  const lmod = join(outDir, `${id}${suffix}.lmod`);
  const tarPath = `${lmod}.tar`;
  await $`tar -cf ${tarPath} -C ${staging} ${entries}`;
  const bytes = Bun.zstdCompressSync(readFileSync(tarPath), { level: 19 });
  writeFileSync(lmod, bytes);
  // A SHA-256 sidecar file so a release/registry consumer can verify integrity.
  writeFileSync(`${lmod}.sha256`, `${Bun.SHA256.hash(bytes, 'hex')}  ${id}${suffix}.lmod\n`);
  rmSync(tarPath, { force: true });
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
