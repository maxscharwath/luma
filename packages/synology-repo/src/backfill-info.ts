#!/usr/bin/env bun
/**
 * One-time backfill: attach a `<spk>.info.json` sidecar to every existing
 * GitHub release that has a .spk but no sidecar yet (new releases get theirs
 * from CI). Downloads each .spk to a temp dir to read INFO + md5, uploads the
 * sidecar, deletes the download. Needs an authenticated `gh`.
 *
 * Usage: bun packages/synology-repo/src/backfill-info.ts [--limit N] [--repo owner/name]
 */
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { readSpkInfo } from './spk';

const args = process.argv.slice(2);
const flag = (name: string) => {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
};
const limit = Number.parseInt(flag('--limit') ?? '1000', 10);
const repo = flag('--repo') ?? 'maxscharwath/kroma';

const gh = (ghArgs: string[]) =>
  execFileSync('gh', [...ghArgs, '--repo', repo], { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });

type Release = { tagName: string; isDraft: boolean };
const releases = (JSON.parse(gh(['release', 'list', '-L', '200', '--json', 'tagName,isDraft'])) as Release[])
  .filter((r) => !r.isDraft)
  .slice(0, limit);

let done = 0;
for (const r of releases) {
  const assets = JSON.parse(gh(['release', 'view', r.tagName, '--json', 'assets'])).assets as { name: string }[];
  const spk = assets.find((a) => a.name.endsWith('.spk'));
  if (!spk) continue;
  if (assets.some((a) => a.name === `${spk.name}.info.json`)) {
    console.log(`${r.tagName}: sidecar already present`);
    continue;
  }
  const dir = mkdtempSync(join(tmpdir(), 'spk-backfill-'));
  try {
    console.log(`${r.tagName}: downloading ${spk.name} ...`);
    gh(['release', 'download', r.tagName, '-p', spk.name, '-D', dir]);
    const info = readSpkInfo(join(dir, spk.name));
    const sidecar = join(dir, `${spk.name}.info.json`);
    writeFileSync(sidecar, `${JSON.stringify({ ...info, beta: r.tagName === 'nightly' }, null, 2)}\n`);
    gh(['release', 'upload', r.tagName, sidecar, '--clobber']);
    console.log(`${r.tagName}: uploaded ${spk.name}.info.json (version ${info.version})`);
    done++;
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
console.log(`Backfilled ${done} release(s).`);
