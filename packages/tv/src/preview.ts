// Samsung Tizen "Smart Hub Preview" integration.
//
// When the LUMA tile is focused on the TV home screen — even with the app NOT
// running — Samsung can expand it into a carousel of content tiles. We surface
// the newest movies there and deep-link straight into playback when a tile is
// selected.
//
// The carousel is rendered by the TV platform from data a *background service*
// provides (config.xml declares `use.preview = bg_service`). That service can't
// reach LUMA on its own (no shared localStorage / mDNS, and Node's `fs` is
// unavailable to it), so the work is split:
//   • the foreground (this file) builds the tile JSON from the live catalog and
//     writes it to the package-private `wgt-private` dir, then nudges the
//     service to republish;
//   • the service (clients/tizen/public/service/preview-service.js) reads that
//     file and calls webapis.preview.setPreviewData(), which the TV shows.
//
// Everything here is feature-detected against the `tizen` global, so it is a
// no-op on webOS and in the browser dev server.

import { type ContinueItem, type LumaClient, type MediaItem, metaLine } from '@luma/core';

// Must match the <tizen:service id> in clients/tizen/public/config.xml.
const SERVICE_ID = 'LumaTV0001.PreviewSvc';
// Package-private dir shared between the foreground app and its service.
const PRIVATE_DIR = 'wgt-private';
const PREVIEW_FILE = 'preview.json';
// Row headers (shown by the carousel) vs. the badge baked onto each card.
const RECENT_SECTION = 'Ajout récent';
const RESUME_SECTION = 'Reprendre la lecture';
const RECENT_BADGE = 'Nouveauté';
const RESUME_BADGE = 'Reprendre';
const MAX_TILES = 20;
// Samsung policy: preview data should not refresh more than ~once per 10 min. We
// keep the on-disk file current on every catalog change but only nudge the
// service to republish past this interval (the TV also polls it on its own).
const REPUBLISH_MS = 10 * 60 * 1000;

/** Action payload carried by a preview tile and handed back to us on launch. */
export interface DeepLink {
  type: 'movie' | 'show';
  id: string;
}

// ---- minimal Tizen typings (foreground web runtime) ------------------------
interface TizenFile {
  resolve(path: string): TizenFile;
  createFile(path: string): TizenFile;
  openStream(
    mode: 'r' | 'w' | 'a' | 'rw',
    onSuccess: (stream: TizenFileStream) => void,
    onError: (e: unknown) => void,
    encoding?: string,
  ): void;
}
interface TizenFileStream {
  write(data: string): void;
  close(): void;
}
interface TizenAppControlData {
  key: string;
  value: string[];
}
interface TizenRequestedAppControl {
  appControl: { operation: string; data: TizenAppControlData[] };
}
interface TizenApp {
  getRequestedAppControl(): TizenRequestedAppControl | null;
}
interface Tizen {
  filesystem: {
    resolve(
      location: string,
      onSuccess: (dir: TizenFile) => void,
      onError: (e: unknown) => void,
      mode?: 'r' | 'rw',
    ): void;
  };
  application: {
    getCurrentApplication(): TizenApp;
    launchAppControl(
      appControl: unknown,
      appId: string | null,
      onSuccess?: () => void,
      onError?: (e: unknown) => void,
      replyCallback?: unknown,
    ): void;
  };
  ApplicationControl: new (operation: string) => unknown;
}

function tizen(): Tizen | null {
  const t = (globalThis as { tizen?: Tizen }).tizen;
  return t?.filesystem && t.application ? t : null;
}

// ---- preview JSON ----------------------------------------------------------

/** Newest first, by ISO-8601 `addedAt`. */
function newest(movies: MediaItem[]): MediaItem[] {
  return [...movies].sort((a, b) => {
    if (a.addedAt < b.addedAt) return 1;
    if (a.addedAt > b.addedAt) return -1;
    return 0;
  });
}

interface Tile {
  // Shown by the carousel itself (the card art carries only the badge + logo).
  title: string;
  subtitle: string;
  image_url: string;
  image_ratio: '16by9';
  action_data: string;
  is_playable: false;
}
interface Section {
  title: string;
  tiles: Tile[];
}

/** True when the server has cached art we can composite a card from. */
function hasArt(m: MediaItem): boolean {
  return !!(m.metadata?.backdropUrl || m.metadata?.posterUrl);
}

/** Where a tile points: movies/videos → their detail page; episodes → the show. */
function deepLinkFor(m: MediaItem): DeepLink {
  return m.kind === 'episode' && m.showId
    ? { type: 'show', id: m.showId }
    : { type: 'movie', id: m.id };
}

/** Native tile title: the show name for episodes, else the item title. */
function titleFor(m: MediaItem): string {
  return m.showTitle ?? m.title;
}

/** Native tile subtitle: media type + the usual meta line (year · runtime · …). */
function subtitleFor(m: MediaItem): string {
  const type = m.kind === 'episode' || m.showId ? 'Série' : 'Film';
  const meta = metaLine(m);
  return meta ? `${type} · ${meta}` : type;
}

/** A landscape "card" tile. The image is the server-composited 16:9 card
 *  (backdrop + category badge + title logo, with an optional resume bar). The
 *  title/subtitle are carousel-native. `?v=<addedAt>` busts the TV's preview
 *  image cache when art changes. */
function tile(client: LumaClient, m: MediaItem, badge: string, progress?: number): Tile {
  const params = new URLSearchParams({ label: badge, v: m.addedAt });
  if (progress != null && progress > 0) params.set('progress', progress.toFixed(3));
  return {
    title: titleFor(m),
    subtitle: subtitleFor(m),
    image_url: `${client.baseUrl}/api/items/${encodeURIComponent(m.id)}/card?${params.toString()}`,
    image_ratio: '16by9',
    action_data: JSON.stringify(deepLinkFor(m)),
    is_playable: false,
  };
}

/** Build the Smart Hub preview document: a "Reprendre la lecture" row (when the
 *  user has resumable items) followed by "Ajout récent" (newest movies). Returns
 *  `null` when there's nothing worth showing. */
export function buildPreviewData(
  client: LumaClient,
  movies: MediaItem[],
  continueItems: ContinueItem[] = [],
): string | null {
  const sections: Section[] = [];

  const resume = continueItems
    .filter((c) => hasArt(c.item))
    .slice(0, MAX_TILES)
    .map((c) =>
      tile(client, c.item, RESUME_BADGE, c.durationMs ? c.positionMs / c.durationMs : undefined),
    );
  if (resume.length) sections.push({ title: RESUME_SECTION, tiles: resume });

  const recent = newest(movies.filter(hasArt))
    .slice(0, MAX_TILES)
    .map((m) => tile(client, m, RECENT_BADGE));
  if (recent.length) sections.push({ title: RECENT_SECTION, tiles: recent });

  return sections.length ? JSON.stringify({ sections }) : null;
}

// ---- file + service plumbing ----------------------------------------------

function writePrivateFile(t: Tizen, name: string, data: string): Promise<void> {
  return new Promise((resolve, reject) => {
    t.filesystem.resolve(
      PRIVATE_DIR,
      (dir) => {
        let file: TizenFile;
        try {
          file = dir.resolve(name);
        } catch {
          file = dir.createFile(name);
        }
        file.openStream(
          'w',
          (stream) => {
            try {
              stream.write(data);
            } finally {
              stream.close();
            }
            resolve();
          },
          reject,
          'UTF-8',
        );
      },
      reject,
      'rw',
    );
  });
}

/** Ask the background service to (re)publish. Best-effort: the TV also polls the
 *  service on its own schedule, so a failure here just delays the refresh. */
function nudgeService(t: Tizen): void {
  try {
    const ctl = new t.ApplicationControl('http://tizen.org/appcontrol/operation/pick');
    t.application.launchAppControl(ctl, SERVICE_ID, undefined, () => undefined);
  } catch {
    /* ignore */
  }
}

let lastNudge = 0;

/** Persist the carousel (resume + recently-added rows) and (throttled) ask the
 *  service to publish it. No-op off Tizen. */
export async function publishPreview(client: LumaClient, movies: MediaItem[]): Promise<void> {
  const t = tizen();
  if (!t) return;
  // Continue-watching is per-user and needs auth — best-effort, empty if absent.
  let continueItems: ContinueItem[] = [];
  try {
    continueItems = await client.continueWatching();
  } catch {
    /* not logged in / no progress yet */
  }
  const data = buildPreviewData(client, movies, continueItems);
  if (!data) return;
  try {
    await writePrivateFile(t, PREVIEW_FILE, data);
    const now = Date.now();
    if (now - lastNudge >= REPUBLISH_MS) {
      lastNudge = now;
      nudgeService(t);
    }
  } catch {
    /* best-effort: the carousel keeps its previous contents */
  }
}

// ---- deep links ------------------------------------------------------------

function asDeepLink(obj: unknown): DeepLink | null {
  if (obj && typeof obj === 'object') {
    const o = obj as Record<string, unknown>;
    if ((o.type === 'movie' || o.type === 'show') && typeof o.id === 'string') {
      return { type: o.type, id: o.id };
    }
  }
  return null;
}

/** Decode a tile's PAYLOAD. The platform delivers our `action_data` either
 *  verbatim, or wrapped as `{"values": "<uri-encoded JSON>"}` (the envelope
 *  Samsung's own sample unwraps) — handle both. */
function parsePayload(raw: string): DeepLink | null {
  try {
    const first = JSON.parse(raw) as unknown;
    const direct = asDeepLink(first);
    if (direct) return direct;
    const values = (first as { values?: unknown })?.values;
    if (typeof values === 'string') {
      return asDeepLink(JSON.parse(decodeURIComponent(values)));
    }
  } catch {
    /* ignore malformed payloads */
  }
  return null;
}

/** The tile selection that launched/targeted the app, or null. */
export function readDeepLink(): DeepLink | null {
  const t = tizen();
  if (!t) return null;
  try {
    const req = t.application.getCurrentApplication().getRequestedAppControl();
    const payload = req?.appControl.data.find((d) => d.key === 'PAYLOAD')?.value?.[0];
    return payload ? parsePayload(payload) : null;
  } catch {
    return null;
  }
}

/** Fire `cb` when the running app is re-targeted by a preview tile. The cold
 *  launch is covered by readDeepLink(); this handles selection while open.
 *  Returns a cleanup function. */
export function onDeepLink(cb: (link: DeepLink) => void): () => void {
  if (!tizen()) return () => undefined;
  const handler = () => {
    const link = readDeepLink();
    if (link) cb(link);
  };
  window.addEventListener('appcontrol', handler);
  return () => window.removeEventListener('appcontrol', handler);
}
