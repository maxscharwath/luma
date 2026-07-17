#!/usr/bin/env bun
/**
 * Emit the `<spk>.info.json` sidecar published next to every .spk release
 * asset. The dynamic package-source worker (packages/synology-repo/worker)
 * aggregates these sidecars from the GitHub Releases API into the live catalog
 * DSM's Package Center queries - no per-release redeploy anywhere.
 *
 * Usage: bun packages/synology-repo/src/gen-spk-info.ts <path/to/pkg.spk> [--beta] [--out <file>]
 * Prints the sidecar path on success.
 */
import { writeFileSync } from 'node:fs';
import { readSpkInfo } from './spk';

const args = process.argv.slice(2);
const spk = args.find((a) => !a.startsWith('--'));
if (!spk) {
  console.error('usage: gen-spk-info.ts <path/to/pkg.spk> [--beta] [--out <file>]');
  process.exit(1);
}
const beta = args.includes('--beta');
const outFlag = args.indexOf('--out');
const out = outFlag >= 0 ? args[outFlag + 1] : `${spk}.info.json`;
if (!out) {
  console.error('--out requires a path');
  process.exit(1);
}

const info = readSpkInfo(spk);
writeFileSync(out, `${JSON.stringify({ ...info, beta }, null, 2)}\n`);
console.log(out);
