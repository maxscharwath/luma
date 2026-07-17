/** Read metadata straight out of a built .spk (a ustar tar), so catalogs can
 * never disagree with the package they describe. Shared by the static catalog
 * generator (gen-catalog.ts) and the per-release sidecar (gen-spk-info.ts) the
 * dynamic package-source worker consumes. */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

const INFO_LINE = /^(\w+)=(.*)$/;

/** Extract a single member of the (ustar) .spk to a Buffer. */
export function extractFromSpk(spk: string, member: string): Buffer {
  return execFileSync('tar', ['-xOf', spk, member], { maxBuffer: 256 * 1024 * 1024 });
}

/** Parse `key="value"` / `key=value` lines from the package's INFO file. */
export function parseInfo(info: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of info.split('\n')) {
    const m = INFO_LINE.exec(line);
    const key = m?.[1];
    const raw = m?.[2];
    if (key === undefined || raw === undefined) continue;
    out[key] = raw.replace(/^"(.*)"$/, '$1');
  }
  return out;
}

/** The store icon bundled in the .spk (256px preferred). */
export function extractIcon(spk: string): Buffer {
  try {
    return extractFromSpk(spk, 'PACKAGE_ICON_256.PNG');
  } catch {
    return extractFromSpk(spk, 'PACKAGE_ICON.PNG');
  }
}

/** Everything a Synology catalog entry needs, read from the .spk itself. */
export type SpkInfo = {
  package: string;
  version: string;
  dname: string;
  desc: string;
  arch: string;
  firmware: string;
  size: number;
  md5: string;
};

export function readSpkInfo(spk: string): SpkInfo {
  const info = parseInfo(extractFromSpk(spk, 'INFO').toString('utf8'));
  const version = info.version;
  if (!version) throw new Error(`No version= in the .spk INFO (${spk})`);
  const bytes = readFileSync(spk);
  return {
    package: info.package || 'kroma',
    version,
    dname: info.displayname || 'KROMA',
    desc: info.description || '',
    arch: info.arch || 'x86_64',
    firmware: info.os_min_ver || '7.0-40000',
    size: bytes.byteLength,
    md5: createHash('md5').update(bytes).digest('hex'),
  };
}
