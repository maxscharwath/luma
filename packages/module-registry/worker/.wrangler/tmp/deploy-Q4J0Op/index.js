var __defProp = Object.defineProperty;
var __name = (target, value) => __defProp(target, "name", { value, configurable: true });

// index.ts
var DEFAULT_REPO = "maxscharwath/kroma";
var CACHE_FRESH = "https://kroma-modules.cache/catalog-fresh";
var CACHE_STALE = "https://kroma-modules.cache/catalog-stale";
var edgeCache = /* @__PURE__ */ __name(() => globalThis.caches?.default, "edgeCache");
async function fetchCatalog(env) {
  const repo = env.GITHUB_REPO || DEFAULT_REPO;
  const headers = { "user-agent": "kroma-module-registry" };
  if (env.GITHUB_TOKEN) headers.authorization = `Bearer ${env.GITHUB_TOKEN}`;
  const res = await fetch(`https://github.com/${repo}/releases/latest/download/modules.json`, {
    headers,
    redirect: "follow"
  });
  if (!res.ok) throw new Error(`modules.json ${res.status}`);
  const body = await res.text();
  return { body, catalog: JSON.parse(body) };
}
__name(fetchCatalog, "fetchCatalog");
async function loadCatalog(env, waitUntil) {
  const cache = edgeCache();
  const hit = await cache?.match(CACHE_FRESH);
  if (hit) {
    const body = await hit.text();
    return { body, catalog: JSON.parse(body) };
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
      return { body, catalog: JSON.parse(body) };
    }
    throw err;
  }
}
__name(loadCatalog, "loadCatalog");
function jsonResponse(body, maxAge) {
  return new Response(body, {
    headers: {
      "content-type": "application/json",
      "cache-control": `public, max-age=${maxAge}`,
      "access-control-allow-origin": "*"
    }
  });
}
__name(jsonResponse, "jsonResponse");
var esc = /* @__PURE__ */ __name((s) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]), "esc");
var mb = /* @__PURE__ */ __name((n) => n ? `${(n / 1048576).toFixed(1)} MB` : "", "mb");
function landing(catalog, origin, repo) {
  const mods = catalog.modules ?? [];
  const rows = mods.map((m) => {
    const targets = (m.artifacts ?? []).map((a) => a.target || "any").join(", ");
    const deps = (m.dependsOn ?? []).length ? `<div class="deps">needs ${(m.dependsOn ?? []).map(esc).join(", ")}</div>` : "";
    const icon = m.icon ? `<img src="${esc(m.icon)}" alt="" />` : '<div class="noicon"></div>';
    return `<div class="mod">
      ${icon}
      <div class="body">
        <div class="row1"><span class="name">${esc(m.name)}</span> <code>${esc(m.version)}</code></div>
        <div class="desc">${esc(m.description ?? "")}</div>
        <div class="meta"><code>${esc(m.id)}</code> \xB7 ${esc(targets)}${m.minServer ? ` \xB7 server \u2265 ${esc(m.minServer)}` : ""} \xB7 ${mb(m.size)}</div>
        ${deps}
      </div>
    </div>`;
  }).join("\n");
  return `<!doctype html>
<html lang="en"><head>
<meta charset="utf-8" /><meta name="viewport" content="width=device-width, initial-scale=1" />
<title>KROMA modules</title>
<style>
  :root { color-scheme: light dark; }
  body { font: 16px/1.6 -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 760px; margin: 6vh auto; padding: 0 20px; }
  h1 { font-size: 1.7rem; } code { background: rgba(127,127,127,.18); padding: .1em .4em; border-radius: 5px; font-size: .85em; }
  .url { display:block; margin:.6em 0 1.6em; padding:.8em 1em; border-radius:10px; background:rgba(127,127,127,.12); font-family:ui-monospace,monospace; word-break:break-all; }
  .mod { display:flex; gap:14px; padding:14px 0; border-bottom:1px solid rgba(127,127,127,.2); }
  .mod img, .mod .noicon { width:48px; height:48px; border-radius:11px; flex:0 0 auto; background:rgba(127,127,127,.15); }
  .row1 .name { font-weight:600; } .desc { opacity:.85; } .meta { font-size:.82em; opacity:.65; margin-top:.2em; } .deps { font-size:.8em; opacity:.6; }
  footer { margin-top:3em; font-size:.85em; opacity:.6; }
  a { color: inherit; }
</style></head><body>
<h1>KROMA modules</h1>
<p>The module store for KROMA. Add this URL as a registry in <b>Admin \u2192 Modules</b>, or browse below.</p>
<code class="url">${origin}/modules.json</code>
<p>${mods.length} module${mods.length === 1 ? "" : "s"} available${catalog.generatedAt ? ` \xB7 updated ${esc(catalog.generatedAt.slice(0, 10))}` : ""}.</p>
${rows || "<p>No modules published yet.</p>"}
<footer>Served live from <a href="https://github.com/${esc(repo)}/releases">github.com/${esc(repo)}</a> \xB7 JSON: <a href="${origin}/modules.json">modules.json</a></footer>
</body></html>`;
}
__name(landing, "landing");
var index_default = {
  async fetch(request, env, ctx) {
    const url = new URL(request.url);
    const path = url.pathname.replace(/\/+$/, "") || "/";
    if (path === "/ping") return new Response("pong");
    let data;
    try {
      data = await loadCatalog(env, (p) => ctx.waitUntil(p));
    } catch (err) {
      return jsonResponse(JSON.stringify({ schema: 2, modules: [], error: String(err) }), 60);
    }
    if (path === "/modules.json" || path === "/all.json") return jsonResponse(data.body, 300);
    const wantsHtml = request.method === "GET" && (request.headers.get("accept") ?? "").includes("text/html");
    if (wantsHtml) {
      return new Response(landing(data.catalog, url.origin, env.GITHUB_REPO || DEFAULT_REPO), {
        headers: { "content-type": "text/html; charset=utf-8", "cache-control": "public, max-age=300" }
      });
    }
    return jsonResponse(data.body, 300);
  }
};
export {
  DEFAULT_REPO,
  index_default as default
};
//# sourceMappingURL=index.js.map
