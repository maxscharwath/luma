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
 * Run:  bun run --filter @kroma/synology-repo gen
 *   or: bun packages/synology-repo/src/gen-catalog.ts
 */
import { existsSync, mkdirSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { channelSubs, renderLanding, type Subs } from './render-landing';
import { extractIcon, readSpkInfo } from './spk';

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

// --- Config (all overridable via env / .env) ---------------------------------
const spk = process.env.CATALOG_SPK?.trim() || findSpk(['.', 'dist', 'clients/synology/dist']);
const downloadUrl = env('CATALOG_DOWNLOAD_URL'); // where DSM downloads the .spk
const pagesUrl = stripTrailingSlash(env('CATALOG_PAGES_URL')); // base URL of the out dir
const outDir = resolve(env('CATALOG_OUT_DIR', 'dist/repo'));
const catalogName = env('CATALOG_NAME', 'catalog.json'); // e.g. nightly.json for a beta channel
const beta = env('CATALOG_BETA', 'false') === 'true';
const meta = {
  maintainer: env('CATALOG_MAINTAINER', 'KROMA'),
  maintainerUrl: env('CATALOG_MAINTAINER_URL', 'https://github.com/maxscharwath/kroma'),
  distributor: env('CATALOG_DISTRIBUTOR', env('CATALOG_MAINTAINER', 'KROMA')),
  distributorUrl: env('CATALOG_DISTRIBUTOR_URL', 'https://github.com/maxscharwath/kroma'),
  changelogUrl: env('CATALOG_CHANGELOG_URL', 'https://github.com/maxscharwath/kroma/releases'),
};

// --- Read the package's own INFO + icon so the catalog can never disagree ------
// DSM compares `version` to the installed version.
const { package: pkg, version: rawVersion, dname, desc, arch, firmware, size, md5 } =
  readSpkInfo(spk);

// build.sh stamps nightlies `X.Y.Z.BUILD-BUILD` (4th feature segment). DSM's
// package-center list hides a package whose feature version has a 4th segment
// that large, so collapse to the conventional `major.minor.micro-build` that it
// renders. Mirrors worker/catalog.ts dsmVersion(). Stable (already 3-segment)
// is untouched.
const [feat = '', build] = rawVersion.split('-');
const version = build ? `${feat.split('.').slice(0, 3).join('.')}-${build}` : feat;

// Icon: an explicit override wins, else pull the store icon out of the .spk itself.
const iconOverride = process.env.CATALOG_ICON?.trim();
const iconBytes = iconOverride ? readFileSync(iconOverride) : extractIcon(spk);

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
      download_count: 0,
      recent_download_count: 0,
      link: downloadUrl,
      size,
      md5,
      thumbnail: [iconUrl],
      thumbnail_retina: [iconUrl],
      snapshot: [],
      maintainer: meta.maintainer,
      maintainer_url: meta.maintainerUrl,
      distributor: meta.distributor,
      distributor_url: meta.distributorUrl,
      changelog: meta.changelogUrl,
      firmware,
      // No `model`/`beta` fields: `model: []` reads to DSM as an empty
      // supported-model whitelist, and `beta: true` from a dynamic source both
      // make DSM silently HIDE the row. SynoCommunity omits both; the channel is
      // gated by which .spk this catalog points at, not a per-package flag. See
      // worker/catalog.ts.
      qinst: true,
      qstart: true,
      qupgrade: true,
      deppkgs: null,
      conflictpkgs: null,
      startable: 'yes',
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
