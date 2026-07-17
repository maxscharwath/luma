/** KROMA dynamic Synology package source - a Cloudflare Worker.
 *
 * Works exactly like SynoCommunity's: paste the BARE worker URL into Package
 * Center → Settings → Package Sources and DSM's POST gets a live, channel- and
 * arch-filtered catalog assembled from the GitHub Releases API (edge-cached
 * 5 min). Publishing a release is all it takes; nothing here redeploys.
 *
 * Routes:
 *   POST /              DSM Package Center (form params: arch, major, package_update_channel, ...)
 *   GET  /              browser landing page listing EVERY release, both channels
 *   GET  /catalog.json  latest stable, DSM catalog shape (static-source compatible)
 *   GET  /nightly.json  nightly channel, DSM catalog shape
 *   GET  /all.json      every entry (machine-readable)
 *   GET  /icon.png      package icon (proxied from the repo)
 *
 * Deploy: bunx wrangler deploy   (packages/synology-repo/worker)
 * Optional secret GITHUB_TOKEN (public-repo read) lifts anonymous API limits.
 */
import {
  archSupported,
  type Catalog,
  cmpDsmVersion,
  DEFAULT_REPO,
  type Entry,
  entryVersion,
  type Env,
  loadCatalog,
  toDsmPackage,
} from './catalog';
import { renderLanding } from './landing';

type ExecCtx = { waitUntil(p: Promise<unknown>): void };

const json = (data: unknown, status = 200) =>
  new Response(JSON.stringify(data, null, 2), {
    status,
    headers: { 'content-type': 'application/json', 'access-control-allow-origin': '*' },
  });

/** DSM query params arrive as an urlencoded POST body; a GET with the same
 * params (older DSMs / manual testing) is honored too. */
async function dsmParams(request: Request, url: URL): Promise<URLSearchParams> {
  if (request.method === 'POST') {
    try {
      const body = await request.text();
      return new URLSearchParams(body);
    } catch {
      return new URLSearchParams();
    }
  }
  return url.searchParams;
}

/** The DSM feed: one entry per channel the requester can see, newest wins. */
function dsmPackages(catalog: Catalog, params: URLSearchParams, origin: string) {
  const arch = params.get('arch');
  if (!archSupported(arch)) return { packages: [] };
  const major = Number.parseInt(params.get('major') ?? '', 10);
  if (!Number.isNaN(major) && major < 7) return { packages: [] }; // DSM 7 floor

  const beta = params.get('package_update_channel') === 'beta';
  const stable = catalog.entries.find((e) => e.channel === 'stable');
  const nightly = catalog.entries.find((e) => e.channel === 'nightly');

  let pick: Entry | undefined = stable;
  if (beta && nightly && (!stable || cmpDsmVersion(entryVersion(nightly), entryVersion(stable)) > 0)) {
    pick = nightly;
  }
  return { packages: pick ? [toDsmPackage(pick, origin, catalog.repo)] : [] };
}

async function icon(repo: string, ctx: ExecCtx): Promise<Response> {
  const src = `https://raw.githubusercontent.com/${repo}/main/clients/synology/spk/PACKAGE_ICON_256.PNG`;
  const cache = (globalThis as unknown as { caches?: { default?: Cache } }).caches?.default;
  const hit = await cache?.match(src);
  if (hit) return hit;
  const res = await fetch(src);
  if (!res.ok) return new Response('icon unavailable', { status: 404 });
  const out = new Response(res.body, {
    headers: { 'content-type': 'image/png', 'cache-control': 'public, max-age=86400' },
  });
  if (cache) ctx.waitUntil(cache.put(src, out.clone()));
  return out;
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecCtx): Promise<Response> {
    const url = new URL(request.url);
    const origin = url.origin;
    const path = url.pathname.replace(/\/+$/, '') || '/';

    if (path === '/ping') return new Response('pong');
    if (path === '/icon.png') return icon(env.GITHUB_REPO || DEFAULT_REPO, ctx);

    let catalog: Catalog;
    try {
      catalog = await loadCatalog(env, (p) => ctx.waitUntil(p));
    } catch (err) {
      return json({ packages: [], error: String(err) }, 503);
    }

    switch (path) {
      case '/catalog.json': {
        const stable = catalog.entries.find((e) => e.channel === 'stable');
        return json({ packages: stable ? [toDsmPackage(stable, origin, catalog.repo)] : [] });
      }
      case '/nightly.json': {
        const nightly = catalog.entries.find((e) => e.channel === 'nightly');
        return json({ packages: nightly ? [toDsmPackage(nightly, origin, catalog.repo)] : [] });
      }
      case '/all.json':
        return json({
          fetchedAt: catalog.fetchedAt,
          repo: catalog.repo,
          packages: catalog.entries.map((e) => ({
            channel: e.channel,
            version: entryVersion(e),
            published: e.publishedAt,
            size: e.info?.size ?? e.spkSize,
            md5: e.info?.md5,
            link: e.spkUrl,
            release: e.releaseUrl,
          })),
        });
      default: {
        // DSM POSTs to whatever base URL the user pasted; browsers GET it.
        const wantsHtml =
          request.method === 'GET' &&
          (request.headers.get('accept') ?? '').includes('text/html') &&
          !url.searchParams.has('arch') &&
          !url.searchParams.has('unique');
        if (wantsHtml) {
          return new Response(renderLanding(catalog, origin), {
            headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'public, max-age=300' },
          });
        }
        return json(dsmPackages(catalog, await dsmParams(request, url), origin));
      }
    }
  },
};
