#!/usr/bin/env bun
/**
 * Generate a Synology "package source" repository for static hosting (GitHub
 * Pages, Cloudflare Pages, any static host).
 *
 * A Synology package source (Package Center > Settings > Package Sources > Add)
 * is just a URL that returns a JSON catalog of packages. Point it at the
 * catalog.json this writes and the package shows up in the Community tab with an
 * Install button + in-place auto-updates - no server. The .spk itself is hosted
 * elsewhere (e.g. a GitHub Release asset); the catalog only points at it.
 *
 * Fully self-contained (reads the version + icon out of the .spk itself) and
 * env-driven so it is not tied to any one repo. Run with `bun`, which auto-loads
 * `.env` from the working directory; CI passes the same vars inline. Uses only
 * Node built-ins, so it also runs under Node. See `.env.example`.
 *
 * Run:  bun run --filter @luma/synology-repo gen
 *   or: bun packages/synology-repo/src/gen-catalog.ts
 */
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { channelSubs, renderLanding, type Subs } from './render-landing';

const INFO_LINE = /^(\w+)=(.*)$/;

/** Read an env var, falling back to a default (or throwing if required). */
function env(key: string, fallback?: string): string {
  const v = process.env[key]?.trim();
  if (v) return v;
  if (fallback !== undefined) return fallback;
  throw new Error(`Missing required env var ${key} (see packages/synology-repo/.env.example)`);
}

/** Strip trailing `/` in one linear pass. A `/\/+$/` regex is super-linear here:
 * unanchored at the start, it retries + backtracks the run at every position. */
function stripTrailingSlash(s: string): string {
  let end = s.length;
  while (end > 0 && s[end - 1] === '/') end--;
  return s.slice(0, end);
}

/** Newest *.spk found across the given dirs (relative to cwd), so a bare run works. */
function findSpk(dirs: string[]): string {
  const found: { path: string; mtime: number }[] = [];
  for (const dir of dirs) {
    const abs = resolve(dir);
    if (!existsSync(abs)) continue;
    for (const f of readdirSync(abs)) {
      if (f.endsWith('.spk'))
        found.push({ path: join(abs, f), mtime: statSync(join(abs, f)).mtimeMs });
    }
  }
  found.sort((a, b) => b.mtime - a.mtime);
  const newest = found[0];
  if (!newest) throw new Error(`No .spk in ${dirs.join(', ')}; set CATALOG_SPK`);
  return newest.path;
}

/** Extract a single member of the (ustar) .spk to a Buffer. */
function extractFromSpk(spk: string, member: string): Buffer {
  return execFileSync('tar', ['-xOf', spk, member], { maxBuffer: 256 * 1024 * 1024 });
}

/** Parse `key="value"` / `key=value` lines from the package's INFO file. */
function parseInfo(info: string): Record<string, string> {
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

// --- Config (all overridable via env / .env) ---------------------------------
const spk = process.env.CATALOG_SPK?.trim() || findSpk(['.', 'dist', 'clients/synology/dist']);
const downloadUrl = env('CATALOG_DOWNLOAD_URL'); // where DSM downloads the .spk
const pagesUrl = stripTrailingSlash(env('CATALOG_PAGES_URL')); // base URL of the out dir
const outDir = resolve(env('CATALOG_OUT_DIR', 'dist/repo'));
const catalogName = env('CATALOG_NAME', 'catalog.json'); // e.g. nightly.json for a beta channel
const beta = env('CATALOG_BETA', 'false') === 'true';
const meta = {
  maintainer: env('CATALOG_MAINTAINER', 'LUMA'),
  maintainerUrl: env('CATALOG_MAINTAINER_URL', 'https://github.com/maxscharwath/luma'),
  distributor: env('CATALOG_DISTRIBUTOR', env('CATALOG_MAINTAINER', 'LUMA')),
  distributorUrl: env('CATALOG_DISTRIBUTOR_URL', 'https://github.com/maxscharwath/luma'),
  changelogUrl: env('CATALOG_CHANGELOG_URL', 'https://github.com/maxscharwath/luma/releases'),
};

// --- Read the package's own INFO + icon so the catalog can never disagree ------
const info = parseInfo(extractFromSpk(spk, 'INFO').toString('utf8'));
const version = info.version; // DSM compares THIS to the installed version
if (!version) throw new Error(`No version= in the .spk INFO (${spk})`);
const pkg = info.package || 'luma';
const dname = info.displayname || 'LUMA';
const desc = info.description || '';
const firmware = info.os_min_ver || '7.0-40000';
const arch = info.arch || 'x86_64';

const spkBytes = readFileSync(spk);
const size = spkBytes.byteLength;
const md5 = createHash('md5').update(spkBytes).digest('hex');

// Icon: an explicit override wins, else pull the store icon out of the .spk itself.
const iconOverride = process.env.CATALOG_ICON?.trim();
const iconBytes = iconOverride ? readFileSync(iconOverride) : tryExtractIcon(spk);

function tryExtractIcon(file: string): Buffer {
  try {
    return extractFromSpk(file, 'PACKAGE_ICON_256.PNG');
  } catch {
    return extractFromSpk(file, 'PACKAGE_ICON.PNG');
  }
}

// --- Emit catalog.json + icon + landing page ---------------------------------
const iconFile = `${pkg}.png`;
const iconUrl = `${pagesUrl}/${iconFile}`;
const catalog = {
  packages: [
    {
      package: pkg,
      version,
      dname,
      desc,
      price: 0,
      download_count: 0,
      recent_download_count: 0,
      link: downloadUrl,
      size,
      md5,
      thumbnail: [iconUrl],
      thumbnail_retina: [iconUrl],
      maintainer: meta.maintainer,
      maintainer_url: meta.maintainerUrl,
      distributor: meta.distributor,
      distributor_url: meta.distributorUrl,
      changelog: meta.changelogUrl,
      firmware,
      beta,
      qinst: true,
      qstart: true,
      qupgrade: true,
      deppkgs: null,
      conflictpkgs: null,
      start: true,
      model: [],
      type: 0,
    },
  ],
};

const catalogUrl = `${pagesUrl}/${catalogName}`;
// Landing page lives in a real HTML file (syntax-highlightable, no escaping); fill
// its {{PLACEHOLDER}} tokens. Values are precomputed so the template stays logic-free.
const subs: Subs = {
  DNAME: dname,
  ICON_FILE: iconFile,
  VERSION: version,
  ARCH: arch,
  DSM_FLOOR: firmware.split('-')[0] ?? firmware,
  CATALOG_URL: catalogUrl,
  DOWNLOAD_URL: downloadUrl,
  ...channelSubs(beta, dname),
};
const template = readFileSync(join(import.meta.dirname, 'landing.template.html'), 'utf8');
const landing = renderLanding(template, subs);

mkdirSync(outDir, { recursive: true });
writeFileSync(join(outDir, catalogName), `${JSON.stringify(catalog, null, 2)}\n`);
writeFileSync(join(outDir, iconFile), iconBytes);
writeFileSync(join(outDir, 'index.html'), landing);

console.log(`Wrote ${dname} ${version}${beta ? ' (beta)' : ''} -> ${outDir}`);
console.log(`  spk:     ${spk}`);
console.log(`  link:    ${downloadUrl}`);
console.log(`  md5:     ${md5}`);
console.log(`  size:    ${size} bytes`);
console.log(`  catalog: ${catalogUrl}`);
