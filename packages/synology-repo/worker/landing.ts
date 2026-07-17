/** Browser-facing landing page: every published version, both channels, plus
 * the copy-paste package-source URL. Styled to match the static Pages landing
 * (packages/synology-repo/src/landing.template.html). */
import { type Catalog, type Entry, entryVersion } from './catalog';

const esc = (s: string) =>
  s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c] as string);

const mb = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MB`;
const day = (iso: string) => (iso ? iso.slice(0, 10) : '');

function row(e: Entry): string {
  return `<tr>
    <td><code>${esc(entryVersion(e))}</code>${e.channel === 'nightly' ? ' <span class="tag">nightly</span>' : ''}</td>
    <td>${day(e.publishedAt)}</td>
    <td>${mb(e.info?.size ?? e.spkSize)}</td>
    <td><a href="${esc(e.spkUrl)}">.spk</a> · <a href="${esc(e.releaseUrl)}">notes</a></td>
  </tr>`;
}

export function renderLanding(catalog: Catalog, origin: string): string {
  const nightly = catalog.entries.find((e) => e.channel === 'nightly');
  const stable = catalog.entries.filter((e) => e.channel === 'stable');
  const latest = stable[0];
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>KROMA - Synology package source</title>
<link rel="icon" href="${origin}/icon.png" />
<style>
  :root { color-scheme: light dark; }
  body { font: 16px/1.6 -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 720px; margin: 6vh auto; padding: 0 20px; }
  h1 { display: flex; align-items: center; gap: 14px; font-size: 1.7rem; }
  h1 img { width: 44px; height: 44px; border-radius: 10px; }
  code { background: rgba(127,127,127,.18); padding: .15em .45em; border-radius: 6px; }
  .url { display: block; margin: .6em 0 1.4em; padding: .8em 1em; border-radius: 10px; background: rgba(127,127,127,.12); font-family: ui-monospace, monospace; word-break: break-all; }
  table { border-collapse: collapse; width: 100%; margin: 1em 0 2em; }
  th, td { text-align: left; padding: .45em .6em; border-bottom: 1px solid rgba(127,127,127,.25); }
  th { font-size: .85em; text-transform: uppercase; letter-spacing: .04em; opacity: .7; }
  .tag { font-size: .75em; padding: .1em .5em; border-radius: 99px; background: rgba(230,160,30,.25); }
  footer { margin-top: 3em; font-size: .85em; opacity: .6; }
  a { color: inherit; }
</style>
</head>
<body>
<h1><img src="${origin}/icon.png" alt="" />KROMA package source</h1>
<p>Self-hosted, direct-play HEVC media streaming for Synology DSM 7 (x86_64).</p>

<h2>Install</h2>
<p>Package Center → <b>Settings</b> → <b>Package Sources</b> → <b>Add</b>, then paste:</p>
<code class="url">${origin}/</code>
<p>KROMA appears in the <b>Community</b> tab and auto-updates with each release.
Enable <b>beta packages</b> (Settings → General → Channel) to ride the nightly channel instead.</p>

${
  latest
    ? `<h2>Latest stable</h2>
<p><code>${esc(entryVersion(latest))}</code> · ${day(latest.publishedAt)} · ${mb(latest.info?.size ?? latest.spkSize)}
· <a href="${esc(latest.spkUrl)}">download .spk</a> · <a href="${esc(latest.releaseUrl)}">release notes</a></p>`
    : '<p>No stable release published yet.</p>'
}
${
  nightly
    ? `<h2>Nightly</h2>
<p><code>${esc(entryVersion(nightly))}</code> · updated ${day(nightly.publishedAt)} · ${mb(nightly.info?.size ?? nightly.spkSize)}
· <a href="${esc(nightly.spkUrl)}">download .spk</a> · <a href="${esc(nightly.releaseUrl)}">release page</a></p>`
    : ''
}

<h2>All releases</h2>
<table>
<thead><tr><th>Version</th><th>Date</th><th>Size</th><th>Links</th></tr></thead>
<tbody>
${catalog.entries.map(row).join('\n')}
</tbody>
</table>

<footer>
Served live from <a href="https://github.com/${esc(catalog.repo)}/releases">github.com/${esc(catalog.repo)}</a>
· catalog refreshed ${esc(catalog.fetchedAt)}
· JSON: <a href="${origin}/catalog.json">stable</a> / <a href="${origin}/nightly.json">nightly</a> / <a href="${origin}/all.json">all</a>
</footer>
</body>
</html>`;
}
