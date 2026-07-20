/** KROMA module registry - a Cloudflare Worker at modules.kroma.tv.
 *
 * Serves the machine-readable module catalog (`modules.json`, schema 2) that the
 * in-app Store reads, and a browser landing page listing every `.kmod` module.
 * The catalog is read live from the latest GitHub Release
 * (`releases/latest/download/modules.json`) and edge-cached, so publishing a
 * release is the whole deploy - nothing here redeploys per module.
 *
 * Routes:
 *   GET /modules.json  the catalog (point the Store's registry URL here)
 *   GET /              browser landing page listing every module; bare URL also
 *                      returns the catalog to non-browser clients
 *   GET /all.json      alias of /modules.json
 *   GET /favicon.svg   the brand mark (also answers /favicon.ico)
 */
import { KROMA_MARK_DATA_URI, KROMA_MARK_SVG } from './brand';

export type Env = {
  /** owner/repo the catalog is published on. */
  GITHUB_REPO?: string;
  /** Optional token to dodge anonymous GitHub rate limits. */
  GITHUB_TOKEN?: string;
};

type Artifact = { target?: string; url: string; size: number; sha256: string };
type ModuleEntry = {
  id: string;
  name: string;
  version: string;
  description?: string;
  minServer?: string;
  dependsOn?: string[];
  icon?: string;
  artifacts?: Artifact[];
  url?: string;
  size?: number;
  sha256?: string;
};
type Catalog = { schema?: number; generatedAt?: string; modules?: ModuleEntry[] };

export const DEFAULT_REPO = 'maxscharwath/kroma';
const CACHE_FRESH = 'https://kroma-modules.cache/catalog-fresh'; // 5 min
const CACHE_STALE = 'https://kroma-modules.cache/catalog-stale'; // 7 days fallback

type ExecCtx = { waitUntil(p: Promise<unknown>): void };
const edgeCache = (): Cache | undefined =>
  (globalThis as unknown as { caches?: { default?: Cache } }).caches?.default;

async function fetchCatalog(env: Env): Promise<{ body: string; catalog: Catalog }> {
  const repo = env.GITHUB_REPO || DEFAULT_REPO;
  const headers: Record<string, string> = { 'user-agent': 'kroma-module-registry' };
  if (env.GITHUB_TOKEN) headers.authorization = `Bearer ${env.GITHUB_TOKEN}`;
  const res = await fetch(`https://github.com/${repo}/releases/latest/download/modules.json`, {
    headers,
    redirect: 'follow',
  });
  if (!res.ok) throw new Error(`modules.json ${res.status}`);
  const body = await res.text();
  return { body, catalog: JSON.parse(body) as Catalog };
}

async function loadCatalog(
  env: Env,
  waitUntil: (p: Promise<unknown>) => void,
): Promise<{ body: string; catalog: Catalog }> {
  const cache = edgeCache();
  const hit = await cache?.match(CACHE_FRESH);
  if (hit) {
    const body = await hit.text();
    return { body, catalog: JSON.parse(body) as Catalog };
  }
  try {
    const { body, catalog } = await fetchCatalog(env);
    if (cache) {
      waitUntil(cache.put(CACHE_FRESH, jsonResponse(body, 300)));
      waitUntil(cache.put(CACHE_STALE, jsonResponse(body, 604800)));
    }
    return { body, catalog };
  } catch (err) {
    const stale = await cache?.match(CACHE_STALE);
    if (stale) {
      const body = await stale.text();
      return { body, catalog: JSON.parse(body) as Catalog };
    }
    throw err;
  }
}

function jsonResponse(body: string, maxAge: number): Response {
  return new Response(body, {
    headers: {
      'content-type': 'application/json',
      'cache-control': `public, max-age=${maxAge}`,
      'access-control-allow-origin': '*',
    },
  });
}

const esc = (s: string) =>
  s.replace(
    /[&<>"]/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c] as string,
  );
const mb = (n?: number) => (n ? `${(n / 1048576).toFixed(1)} MB` : '');

/** The brand mark, served as a real SVG so the tab icon is the current logo. */
function favicon(): Response {
  return new Response(KROMA_MARK_SVG, {
    headers: {
      'content-type': 'image/svg+xml',
      // Short: a brand change must not stay pinned at the edge for a day.
      'cache-control': 'public, max-age=3600',
    },
  });
}

function landing(catalog: Catalog, origin: string, repo: string): string {
  const mods = catalog.modules ?? [];
  const rows = mods
    .map((m) => {
      const targets = (m.artifacts ?? []).map((a) => a.target || 'any').join(', ');
      const deps = (m.dependsOn ?? []).length
        ? `<div class="deps">needs ${(m.dependsOn ?? []).map(esc).join(', ')}</div>`
        : '';
      const icon = m.icon ? `<img src="${esc(m.icon)}" alt="" />` : '<div class="noicon"></div>';
      return `<div class="mod">
      ${icon}
      <div class="body">
        <div class="row1"><span class="name">${esc(m.name)}</span> <code>${esc(m.version)}</code></div>
        <div class="desc">${esc(m.description ?? '')}</div>
        <div class="meta"><code>${esc(m.id)}</code> · ${esc(targets)}${m.minServer ? ` · server ≥ ${esc(m.minServer)}` : ''} · ${mb(m.size)}</div>
        ${deps}
      </div>
    </div>`;
    })
    .join('\n');
  return `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" />
<title>KROMA modules</title>
<link rel="icon" href="${KROMA_MARK_DATA_URI}" />
<style>
  :root { color-scheme: light dark; }
  h1 { display:flex; align-items:center; gap:14px; }
  h1 svg { width:38px; height:38px; flex:0 0 auto; }
  body { font: 16px/1.6 -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 760px; margin: 6vh auto; padding: 0 20px; }
  h1 { font-size: 1.7rem; } code { background: rgba(127,127,127,.18); padding: .1em .4em; border-radius: 5px; font-size: .85em; }
  .url { display:block; margin:.6em 0 1.6em; padding:.8em 1em; border-radius:10px; background:rgba(127,127,127,.12); font-family:ui-monospace,monospace; word-break:break-all; }
  .mod { display:flex; gap:14px; padding:14px 0; border-bottom:1px solid rgba(127,127,127,.2); }
  .mod img, .mod .noicon { width:48px; height:48px; border-radius:11px; flex:0 0 auto; background:rgba(127,127,127,.15); }
  .row1 .name { font-weight:600; } .desc { opacity:.85; } .meta { font-size:.82em; opacity:.65; margin-top:.2em; } .deps { font-size:.8em; opacity:.6; }
  footer { margin-top:3em; font-size:.85em; opacity:.6; }
  a { color: inherit; }
</style></head><body>
<h1>${KROMA_MARK_SVG}KROMA modules</h1>
<p>The module store for KROMA. Add this URL as a registry in <b>Admin → Modules</b>, or browse below.</p>
<code class="url">${origin}/modules.json</code>
<p>${mods.length} module${mods.length === 1 ? '' : 's'} available${catalog.generatedAt ? ` · updated ${esc(catalog.generatedAt.slice(0, 10))}` : ''}.</p>
${rows || '<p>No modules published yet.</p>'}
<footer>Served live from <a href="https://github.com/${esc(repo)}/releases">github.com/${esc(repo)}</a> · JSON: <a href="${origin}/modules.json">modules.json</a></footer>
</body></html>`;
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecCtx): Promise<Response> {
    const url = new URL(request.url);
    const path = url.pathname.replace(/(^|[^/])\/+$/, '$1') || '/';
    if (path === '/ping') return new Response('pong');
    // Before the catalog load: these must not fall through to the JSON
    // catch-all below, which used to answer /favicon.ico with 200 + the whole
    // modules.json (so browsers kept whatever icon they had cached).
    if (path === '/favicon.svg' || path === '/favicon.ico') return favicon();

    let data: { body: string; catalog: Catalog };
    try {
      data = await loadCatalog(env, (p) => ctx.waitUntil(p));
    } catch (err) {
      return jsonResponse(JSON.stringify({ schema: 2, modules: [], error: String(err) }), 60);
    }

    if (path === '/modules.json' || path === '/all.json') return jsonResponse(data.body, 300);

    const wantsHtml =
      request.method === 'GET' && (request.headers.get('accept') ?? '').includes('text/html');
    if (wantsHtml) {
      return new Response(landing(data.catalog, url.origin, env.GITHUB_REPO || DEFAULT_REPO), {
        headers: {
          'content-type': 'text/html; charset=utf-8',
          'cache-control': 'public, max-age=300',
        },
      });
    }
    return jsonResponse(data.body, 300);
  },
};
