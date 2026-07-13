// Shared module-bundle builder: compile the WASM backend guest, build the Module
// Federation frontend remote, and stage the installable payload (module.json +
// optional module.wasm + icon + fe/). The archive step (raw `.tar` vs gzipped
// `.lmod`) is left to the caller; this does everything up to that point.

import {
  copyFileSync,
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
} from 'node:fs';
import { join } from 'node:path';
import { $ } from 'bun';

export interface StagedBundle {
  /** The module id from its `module.json`. */
  id: string;
  /** Temp dir holding the bundle's files (delete after archiving). */
  staging: string;
  /** Top-level entries to archive, in a stable order (no `./` prefix). */
  entries: string[];
}

/** Build + stage a module's installable payload. Throws if a declared backend
 *  produced no `.wasm` or a declared frontend produced no `dist/`. */
export async function buildModuleBundle(moduleDir: string): Promise<StagedBundle> {
  const manifest = JSON.parse(readFileSync(join(moduleDir, 'module.json'), 'utf8')) as {
    id: string;
  };
  const { id } = manifest;
  console.log(`building module bundle: ${id}`);

  // 1) Compile the WASM backend guest (if the module ships one).
  const beDir = join(moduleDir, 'server');
  if (existsSync(beDir)) {
    console.log('  - cargo build --target wasm32-unknown-unknown');
    await $`cargo build --release --target wasm32-unknown-unknown`.cwd(beDir);
  }
  const wasmOutDir = join(beDir, 'target/wasm32-unknown-unknown/release');
  const wasmCandidates = existsSync(wasmOutDir)
    ? readdirSync(wasmOutDir)
        .filter((f) => f.endsWith('.wasm'))
        .sort()
    : [];
  // A module that ships a backend but produced no .wasm did not actually build
  // (e.g. the guest crate is not `crate-type = ["cdylib"]`). Fail loudly rather
  // than shipping a backend-less bundle that 404s on every /api/plugin call.
  if (existsSync(beDir) && wasmCandidates.length === 0) {
    throw new Error(
      `no .wasm produced in ${wasmOutDir}; is the guest crate crate-type = ["cdylib"]?`,
    );
  }
  if (wasmCandidates.length > 1) {
    console.warn(
      `  ! multiple .wasm found, using ${wasmCandidates[0]}: ${wasmCandidates.join(', ')}`,
    );
  }
  const wasmFile = wasmCandidates[0];

  // 2) Build the Module Federation frontend remote (if the module ships one).
  const feDir = join(moduleDir, 'ui');
  const feDist = join(feDir, 'dist');
  if (existsSync(feDir)) {
    console.log('  - vite build (frontend remote)');
    await $`bun run build`.cwd(feDir);
    if (!existsSync(feDist)) {
      throw new Error(
        `ui/ built but produced no dist/ at ${feDist}; the module page would be missing`,
      );
    }
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

  return { id, staging, entries };
}
