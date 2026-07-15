#!/usr/bin/env bun

// Build a static module registry from the packed `.lmod` files: a catalog
// index (schema 2) plus the `.lmod` files themselves, ready to publish to any
// static host. The canonical deployment is the release workflow, which attaches
// the .lmod files + this index (as `modules.json`) to the GitHub Release, so
// `https://github.com/<owner>/<repo>/releases/latest/download/modules.json` is
// a permanent, always-current store URL. The in-app Store fetches the index,
// picks the artifact matching the server's build target, verifies its sha256,
// and resolves `dependsOn` before installing.
//
//   bun run scripts/gen-registry.ts                             # from dist/modules/*.lmod
//   bun run scripts/gen-registry.ts --base https://mods.example.com
//   bun run scripts/gen-registry.ts --base .../releases/download/v0.1.5
//
// Output: dist/registry/{catalog.json, <id>[-<target>].lmod, ...}
//
// Catalog schema 2: one entry per module id with `artifacts` grouped per build
// target (a sidecar .lmod carries a native binary, so CI packs one per target
// and suffixes the filename with the triple; library modules are unsuffixed and
// platform-independent). Flat url/size/sha256 of the first artifact are kept
// per entry so schema-1 consumers keep working.

import { createHash } from 'node:crypto';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';

const root = join(import.meta.dir, '..');
const modulesDir = join(root, 'dist/modules');
const outDir = join(root, 'dist/registry');

const baseIdx = process.argv.indexOf('--base');
const baseUrl = baseIdx >= 0 ? process.argv[baseIdx + 1].replace(/\/$/, '') : '';

if (!existsSync(modulesDir)) {
  throw new Error(`no packed modules at ${modulesDir}; run \`bun run modules:pack\` first`);
}
mkdirSync(outDir, { recursive: true });

interface Manifest {
  id: string;
  name: string;
  version: string;
  description?: string;
  minServer?: string;
  library?: boolean;
  dependsOn?: Record<string, string> | unknown[];
}

interface Artifact {
  /** Build-target triple of the native binary, or null = platform-independent. */
  target: string | null;
  file: string;
  url: string;
  size: number;
  sha256: string;
}

interface Entry {
  id: string;
  name: string;
  version: string;
  description?: string;
  minServer?: string;
  library?: boolean;
  dependsOn?: Record<string, string> | unknown[];
  artifacts: Artifact[];
  // Schema-1 compatibility mirror of artifacts[0].
  file: string;
  url: string;
  size: number;
  sha256: string;
}

/** Decompress a `.lmod` to its inner tar (zstd today; gzip/raw accepted like
 *  the server's installer, dispatched by magic bytes). */
function toTar(bytes: Buffer): Uint8Array {
  if (bytes[0] === 0x28 && bytes[1] === 0xb5 && bytes[2] === 0x2f && bytes[3] === 0xfd) {
    return Bun.zstdDecompressSync(bytes);
  }
  if (bytes[0] === 0x1f && bytes[1] === 0x8b) {
    return Bun.gunzipSync(bytes);
  }
  return bytes;
}

/** Read one file out of a (ustar) tar buffer. Headers are 512-byte blocks:
 *  name at 0..100 (NUL-padded), size as octal at 124..136. */
function tarRead(tar: Uint8Array, wanted: string): Uint8Array | null {
  const field = (start: number, len: number) =>
    new TextDecoder().decode(tar.subarray(start, start + len)).split('\0')[0];
  let off = 0;
  while (off + 512 <= tar.length) {
    const name = field(off, 100);
    if (!name) break; // two empty blocks terminate the archive
    const size = Number.parseInt(field(off + 124, 12).trim() || '0', 8);
    if (name === wanted || name === `./${wanted}`) {
      return tar.subarray(off + 512, off + 512 + size);
    }
    off += 512 + Math.ceil(size / 512) * 512;
  }
  return null;
}

const lmods = readdirSync(modulesDir).filter((f) => f.endsWith('.lmod'));
const entries = new Map<string, Entry>();

for (const file of lmods.sort()) {
  const path = join(modulesDir, file);
  const bytes = readFileSync(path);
  const manifestBytes = tarRead(toTar(bytes), 'module.json');
  if (!manifestBytes) {
    console.warn(`  ! ${file}: no module.json inside, skipped`);
    continue;
  }
  const manifest = JSON.parse(new TextDecoder().decode(manifestBytes)) as Manifest;

  // The pack script names bundles `<id>.lmod` (host/library build) or
  // `<id>-<target>.lmod` (per-target sidecar); recover the target from the
  // filename using the id we just read out of the bundle.
  const stem = file.slice(0, -'.lmod'.length);
  const target =
    stem === manifest.id ? null : stem.slice(manifest.id.length).replace(/^-/, '') || null;

  copyFileSync(path, join(outDir, file));
  const artifact: Artifact = {
    target,
    file,
    url: baseUrl ? `${baseUrl}/${file}` : file,
    size: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
  };

  const existing = entries.get(manifest.id);
  if (existing) {
    existing.artifacts.push(artifact);
  } else {
    entries.set(manifest.id, {
      id: manifest.id,
      name: manifest.name,
      version: manifest.version,
      description: manifest.description,
      minServer: manifest.minServer,
      library: manifest.library,
      dependsOn: manifest.dependsOn,
      artifacts: [artifact],
      file: artifact.file,
      url: artifact.url,
      size: artifact.size,
      sha256: artifact.sha256,
    });
  }
}

const modules = [...entries.values()].sort((a, b) => a.id.localeCompare(b.id));
for (const m of modules) {
  m.artifacts.sort((a, b) => (a.target ?? '').localeCompare(b.target ?? ''));
}
const catalog = { schema: 2, generatedAt: new Date().toISOString(), modules };
writeFileSync(join(outDir, 'catalog.json'), `${JSON.stringify(catalog, null, 2)}\n`);

console.log(`registry: ${modules.length} module(s) -> ${outDir}/catalog.json`);
for (const m of modules) {
  const targets = m.artifacts.map((a) => a.target ?? 'universal').join(', ');
  console.log(`  ${m.id}  v${m.version}  [${targets}]`);
}
