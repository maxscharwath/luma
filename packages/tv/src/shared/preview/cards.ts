// Smart Hub preview card-builder: turns the live catalog (newest movies +
// resumable items) into the carousel tile JSON the TV expects.

import { type ContinueItem, type KromaClient, type MediaItem, metaLine } from '@kroma/core';
import type { DeepLink } from '#tv/shared/preview/types';

// Row headers (shown by the carousel) vs. the badge baked onto each card.
const RECENT_SECTION = 'Ajout récent';
const RESUME_SECTION = 'Reprendre la lecture';
const RECENT_BADGE = 'Nouveauté';
const RESUME_BADGE = 'Reprendre';
const MAX_TILES = 20;

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
function tile(client: KromaClient, m: MediaItem, badge: string, progress?: number): Tile {
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
  client: KromaClient,
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
