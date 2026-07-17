/** Live catalog assembly for the dynamic Synology package source.
 *
 * Reads the GitHub Releases list (+ the `<spk>.info.json` sidecars CI attaches
 * next to every .spk) and turns it into channel-aware catalog entries. Nothing
 * is rebuilt or redeployed per release: publishing a release IS the deploy.
 * Results are edge-cached for 5 minutes, with a week-long stale copy served if
 * the GitHub API is unreachable.
 */

export type Env = {
  /** owner/repo to read releases from. */
  GITHUB_REPO?: string;
  /** Optional fine-grained token (public repo read) to dodge anonymous API rate limits. */
  GITHUB_TOKEN?: string;
};

/** Fields of the `<spk>.info.json` sidecar (packages/synology-repo/src/gen-spk-info.ts). */
export type SpkInfo = {
  package: string;
  version: string;
  dname: string;
  desc: string;
  arch: string;
  firmware: string;
  size: number;
  md5: string;
  beta: boolean;
};

export type Entry = {
  channel: 'stable' | 'nightly';
  tag: string;
  releaseName: string;
  releaseUrl: string;
  publishedAt: string;
  spkName: string;
  spkUrl: string;
  spkSize: number;
  info: SpkInfo | null;
};

export type Catalog = {
  fetchedAt: string;
  repo: string;
  entries: Entry[];
};

type GhAsset = { name: string; size: number; browser_download_url: string };
type GhRelease = {
  tag_name: string;
  name: string | null;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  html_url: string;
  assets: GhAsset[];
};

export const DEFAULT_REPO = 'maxscharwath/kroma';
const CACHE_FRESH = 'https://kroma-packages.cache/catalog-fresh'; // 5 min
const CACHE_STALE = 'https://kroma-packages.cache/catalog-stale'; // 7 days, disaster fallback
const MAX_SIDECARS = 60; // newest releases whose sidecar we bother fetching

const edgeCache = (): Cache | undefined =>
  (globalThis as unknown as { caches?: { default?: Cache } }).caches?.default;

function ghHeaders(env: Env): HeadersInit {
  const h: Record<string, string> = {
    'user-agent': 'kroma-package-source-worker',
    accept: 'application/vnd.github+json',
    'x-github-api-version': '2022-11-28',
  };
  if (env.GITHUB_TOKEN) h.authorization = `Bearer ${env.GITHUB_TOKEN}`;
  return h;
}

async function fetchCatalogFromGitHub(env: Env): Promise<Catalog> {
  const repo = env.GITHUB_REPO || DEFAULT_REPO;
  const res = await fetch(`https://api.github.com/repos/${repo}/releases?per_page=100`, {
    headers: ghHeaders(env),
  });
  if (!res.ok) throw new Error(`GitHub releases API ${res.status}`);
  const releases = (await res.json()) as GhRelease[];

  const entries: Entry[] = [];
  for (const r of releases) {
    if (r.draft) continue;
    const spk = r.assets.find((a) => a.name.endsWith('.spk'));
    if (!spk) continue; // desktop-latest & friends carry no package
    const channel = r.tag_name === 'nightly' ? 'nightly' : r.prerelease ? null : 'stable';
    if (!channel) continue;
    entries.push({
      channel,
      tag: r.tag_name,
      releaseName: r.name || r.tag_name,
      releaseUrl: r.html_url,
      publishedAt: r.published_at || '',
      spkName: spk.name,
      spkUrl: spk.browser_download_url,
      spkSize: spk.size,
      info: null,
    });
  }

  // Sidecars in parallel; entries without one (pre-sidecar releases) stay usable
  // on the landing page via the filename-derived version.
  await Promise.all(
    entries.slice(0, MAX_SIDECARS).map(async (e) => {
      const rel = releases.find((r) => r.tag_name === e.tag);
      const sidecar = rel?.assets.find((a) => a.name === `${e.spkName}.info.json`);
      if (!sidecar) return;
      try {
        const res = await fetch(sidecar.browser_download_url, { headers: { 'user-agent': 'kroma-package-source-worker' } });
        if (res.ok) e.info = (await res.json()) as SpkInfo;
      } catch {
        // tolerate a missing/broken sidecar; the entry just loses md5/desc
      }
    }),
  );

  // Newest first: nightly entry (if any) leads, then stable by publish date.
  entries.sort((a, b) => {
    if (a.channel !== b.channel) return a.channel === 'nightly' ? -1 : 1;
    return b.publishedAt.localeCompare(a.publishedAt);
  });
  return { fetchedAt: new Date().toISOString(), repo, entries };
}

/** Cached catalog: 5-minute edge cache, refreshed inline on miss; a week-long
 * stale copy answers if GitHub is down or rate-limits the anonymous fetch. */
export async function loadCatalog(env: Env, waitUntil: (p: Promise<unknown>) => void): Promise<Catalog> {
  const cache = edgeCache();
  const hit = await cache?.match(CACHE_FRESH);
  if (hit) return (await hit.json()) as Catalog;
  try {
    const catalog = await fetchCatalogFromGitHub(env);
    const body = JSON.stringify(catalog);
    if (cache) {
      waitUntil(cache.put(CACHE_FRESH, jsonResponse(body, 300)));
      waitUntil(cache.put(CACHE_STALE, jsonResponse(body, 604800)));
    }
    return catalog;
  } catch (err) {
    const stale = await cache?.match(CACHE_STALE);
    if (stale) return (await stale.json()) as Catalog;
    throw err;
  }
}

function jsonResponse(body: string, maxAge: number): Response {
  return new Response(body, {
    headers: { 'content-type': 'application/json', 'cache-control': `public, max-age=${maxAge}` },
  });
}

/** `kroma-0.1.25-3439372-x86_64.spk` -> `0.1.25-3439372` (fallback when a
 * release predates the .info.json sidecars). */
export function versionFromSpkName(name: string): string {
  const m = /^kroma-(.+)-x86_64\.spk$/.exec(name);
  return m?.[1] ?? name.replace(/\.spk$/, '');
}

export function entryVersion(e: Entry): string {
  return e.info?.version ?? versionFromSpkName(e.spkName);
}

/** DSM's version ordering: compare the dotted feature version numerically
 * segment by segment, then the -build suffix. */
export function cmpDsmVersion(a: string, b: string): number {
  const parse = (v: string) => {
    const [feat = '', build = '0'] = v.split('-');
    return { seg: feat.split('.').map((n) => Number.parseInt(n, 10) || 0), build: Number.parseInt(build, 10) || 0 };
  };
  const pa = parse(a);
  const pb = parse(b);
  for (let i = 0; i < Math.max(pa.seg.length, pb.seg.length); i++) {
    const d = (pa.seg[i] ?? 0) - (pb.seg[i] ?? 0);
    if (d !== 0) return d;
  }
  return pa.build - pb.build;
}

/** DSM arch codenames covered by our single x86_64 build (spksrc x64 families). */
const X86_64_ARCHES = new Set([
  'x86_64',
  'x64',
  'apollolake',
  'avoton',
  'braswell',
  'broadwell',
  'broadwellnk',
  'broadwellnkv2',
  'broadwellntbap',
  'bromolow',
  'cedarview',
  'denverton',
  'geminilake',
  'grantley',
  'icelaked',
  'kvmx64',
  'purley',
  'v1000',
  'r1000',
  'epyc7002',
]);

export function archSupported(arch: string | null): boolean {
  if (!arch) return true; // no arch reported: stay permissive
  return X86_64_ARCHES.has(arch.toLowerCase()) || arch.toLowerCase() === 'noarch';
}

/** One catalog entry -> the JSON object DSM's Package Center expects (same
 * shape as gen-catalog.ts / SynoCommunity's spkrepo). */
export function toDsmPackage(e: Entry, origin: string, repo: string) {
  const info = e.info;
  return {
    package: info?.package ?? 'kroma',
    version: entryVersion(e),
    dname: info?.dname ?? 'KROMA',
    desc: info?.desc ?? 'KROMA - self-hosted, direct-play HEVC media streaming.',
    price: 0,
    download_count: 0,
    recent_download_count: 0,
    link: e.spkUrl,
    size: info?.size ?? e.spkSize,
    md5: info?.md5,
    thumbnail: [`${origin}/icon.png`],
    thumbnail_retina: [`${origin}/icon.png`],
    maintainer: 'KROMA',
    maintainer_url: `https://github.com/${repo}`,
    distributor: 'KROMA',
    distributor_url: `https://github.com/${repo}`,
    changelog: e.releaseUrl,
    firmware: info?.firmware ?? '7.0-40000',
    beta: e.channel === 'nightly',
    qinst: true,
    qstart: true,
    qupgrade: true,
    deppkgs: null,
    conflictpkgs: null,
    start: true,
    model: [],
    type: 0,
  };
}
