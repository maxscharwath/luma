#!/usr/bin/env bun

// Emit the `cargo build` commands that produce every sidecar module's native
// binary, one per line. CI runs these INSIDE the musl cross-toolchain image
// (messense/rust-musl-cross, the same image the Synology .spk build uses, which
// is proven to link candle / librqbit for musl), then packs the results on the
// host with `KMOD_SKIP_BUILD=1 bun run modules:pack`. Keeping the plan derived
// from each module's Cargo.toml (via the shared pack-module helpers) means new
// modules and feature changes need no workflow edit.
//
//   KMOD_TARGET=x86_64-unknown-linux-musl bun run scripts/kmod-build-plan.ts
//
// Output (stdout): shell command lines. Library modules (no [[bin]]) are skipped.

import { crateAndBin, packableModules } from './pack-module';

const target = process.env.KMOD_TARGET?.trim();
const targetArg = target ? ` --target ${target}` : '';

const lines: string[] = [];
for (const dir of packableModules()) {
  const { pkg, bin, features } = crateAndBin(dir);
  if (!bin) continue; // library module: nothing to compile
  const feat = features.length ? ` --features ${features.join(',')}` : '';
  lines.push(`cargo build --profile release-kmod -p ${pkg} --bin ${bin}${feat}${targetArg}`);
}

// One combined line keeps cargo's dependency graph warm across modules while
// still honoring per-crate features (cargo unifies features workspace-wide).
process.stdout.write(`${lines.join('\n')}\n`);
