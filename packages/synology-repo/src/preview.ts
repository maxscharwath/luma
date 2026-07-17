#!/usr/bin/env bun
/**
 * Live-reload preview of the package-source landing page. Edit
 * `landing.template.html` and the browser reloads on save - no .spk or build
 * needed (it renders the template with realistic sample values).
 *
 * Run:  bun run --filter @kroma/synology-repo preview
 *   or: bun packages/synology-repo/src/preview.ts
 * Env:  PORT (default 4321), CATALOG_BETA=true to preview the nightly variant.
 */
import { readFileSync, watch } from 'node:fs';
import { createServer, type ServerResponse } from 'node:http';
import { join } from 'node:path';
import { channelSubs, renderLanding, type Subs } from './render-landing';

const dir = import.meta.dirname;
const templateFile = 'landing.template.html';
const templatePath = join(dir, templateFile);
const port = Number(process.env.PORT ?? 4321);
const beta = process.env.CATALOG_BETA === 'true';

// Inline placeholder icon so the preview stays self-contained (no reach into a
// sibling client's assets); the real page uses the icon from inside the .spk.
const ICON =
  "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 64 64'%3E%3Ccircle cx='32' cy='32' r='30' fill='%23d9a521'/%3E%3Ccircle cx='32' cy='32' r='9' fill='%231a1a1a'/%3E%3C/svg%3E";

// Realistic sample values so the preview looks like a real published page.
const sampleSubs: Subs = {
  DNAME: 'KROMA',
  ICON_FILE: ICON,
  VERSION: '0.1.4.3431188-3431188',
  ARCH: 'x86_64',
  DSM_FLOOR: '7.0',
  CATALOG_URL: `https://maxscharwath.github.io/kroma/${beta ? 'nightly.json' : 'catalog.json'}`,
  DOWNLOAD_URL: 'https://github.com/maxscharwath/kroma/releases/latest',
  ...channelSubs(beta, 'KROMA'),
};

const LIVE_RELOAD = `<script>new EventSource('/__reload').onmessage = () => location.reload();</script>`;
const clients = new Set<ServerResponse>();

const server = createServer((req, res) => {
  const path = (req.url ?? '/').split('?', 1)[0] ?? '/';
  if (path === '/__reload') {
    res.writeHead(200, {
      'content-type': 'text/event-stream',
      'cache-control': 'no-cache',
      connection: 'keep-alive',
    });
    res.write(':\n\n'); // open the stream
    clients.add(res);
    req.on('close', () => clients.delete(res));
    return;
  }
  const html = renderLanding(readFileSync(templatePath, 'utf8'), sampleSubs);
  res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
  res.end(html + LIVE_RELOAD);
});

// Watch the dir (survives editors' atomic saves) and push a reload on template edits.
watch(dir, (_event, filename) => {
  if (filename !== templateFile) return;
  for (const res of clients) res.write('data: reload\n\n');
});

server.listen(port, () => {
  console.log(`Preview  http://localhost:${port}`);
  console.log(
    '  edit src/landing.template.html to hot-reload; CATALOG_BETA=true for the nightly variant',
  );
});
