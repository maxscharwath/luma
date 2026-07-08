// One normalized "title" model that both the library fiche (owned) and the
// discover fiche (TMDB / request flow) render through, so a movie/show is one
// page instead of two divergent stacks. Built client-side from either source
// (or both, for a partially-owned show): local playback drives Play, the TMDB
// overlay drives Request + per-season availability.

import {
  type CastMember,
  type CrewMember,
  type DiscoverDetail,
  type DiscoverEntry,
  formatRuntime,
  hasPermission,
  type LumaClient,
  type MediaItem,
  type RequestStatus,
  type Season,
  type Show,
  type ShowDetail,
  type Translate,
  type UpNext,
  type User,
  type VideoTrack,
} from '@luma/core';
import { imageUrl } from '#web/shared/lib/api';

/** A season in the unified model: owned playable episodes merged with TMDB
 * availability, so one list renders play-owned + request-missing. */
export interface TitleSeason {
  number: number;
  name: string | null;
  /** TMDB episode count (0 = unknown / local-only). */
  episodeCount: number;
  episodesAvailable: number;
  available: boolean;
  requested: boolean;
  /** Owned, playable episodes (empty for a not-owned season). */
  episodes: MediaItem[];
  cast: CastMember[];
}

/** A "similar" tile that knows where it opens (local fiche vs discover). */
export interface SimilarTarget {
  key: string;
  title: string;
  poster: string;
  genre: string;
  localId: string | null;
  tmdbId: number | null;
  kind: 'movie' | 'show';
}

export interface TitleView {
  kind: 'movie' | 'show';
  tmdbId: number | null;
  localId: string | null;
  inLibrary: boolean;
  title: string;
  year: number | null;
  rating: number | null;
  overview: string | null;
  tagline: string | null;
  genres: string[];
  /** TMDB runtime for a not-owned movie's meta line (owned uses the file duration). */
  runtimeMin: number | null;
  poster: string;
  backdrop: string | null;
  directors: string[];
  cast: CastMember[];
  themeUrl: string | null;
  /** Representative video for the quality badges (null = not owned). */
  video: VideoTrack | null;
  /** The item Play targets (movie item, or a show's up-next/first episode). */
  playable: MediaItem | null;
  playLabel: string | null;
  seasons: TitleSeason[];
  requestStatus: RequestStatus | null;
  requestProgress: number | null;
  /** The viewer may request missing parts (TMDB id known + `requests.create`). */
  canRequest: boolean;
  similar: SimilarTarget[];
}

function dirsFromCrew(crew: CrewMember[] | null | undefined): string[] {
  return (crew ?? []).filter((c) => c.job === 'Director' || c.job === 'Creator').map((c) => c.name);
}

/** Every input a title page can arrive with. `discover` overlays availability +
 * request state onto owned content (null when unavailable / no permission). */
export type TitleInput =
  | { source: 'movie'; item: MediaItem; similar: MediaItem[]; discover: DiscoverDetail | null }
  | {
      source: 'show';
      detail: ShowDetail;
      similarShows: Show[];
      upNext: UpNext | null;
      discover: DiscoverDetail | null;
    }
  | { source: 'discover'; detail: DiscoverDetail };

export function buildTitleView(
  c: LumaClient,
  t: Translate,
  user: User | null,
  input: TitleInput,
): TitleView {
  const canReq = (tmdbId: number | null, hasDiscover: boolean) =>
    hasDiscover && tmdbId != null && !!user && hasPermission(user, 'requests.create');

  if (input.source === 'movie') {
    const { item, similar, discover } = input;
    const meta = item.metadata ?? null;
    return {
      kind: 'movie',
      tmdbId: meta?.tmdbId ?? null,
      localId: item.id,
      inLibrary: true,
      title: item.title,
      year: item.year ?? null,
      rating: meta?.rating ?? null,
      overview: meta?.overview ?? null,
      tagline: meta?.tagline ?? null,
      genres: meta?.genres ?? [],
      runtimeMin: null,
      poster: c.posterFor(item),
      backdrop: c.backdropFor(item),
      directors: dirsFromCrew(meta?.crew),
      cast: meta?.cast ?? [],
      themeUrl: null,
      video: item.video,
      playable: item,
      playLabel: null,
      seasons: [],
      requestStatus: discover?.requestStatus ?? null,
      requestProgress: discover?.requestProgress ?? null,
      canRequest: false,
      similar: similar.map((m) => ({
        key: m.id,
        title: m.title,
        poster: c.posterFor(m),
        genre: m.metadata?.genres?.[0] ?? t('content.film'),
        localId: m.id,
        tmdbId: null,
        kind: 'movie' as const,
      })),
    };
  }

  if (input.source === 'show') {
    const { detail, similarShows, upNext, discover } = input;
    const show = detail.show;
    const meta = show.metadata ?? null;
    const first = detail.seasons[0]?.episodes[0] ?? null;
    const playTarget = upNext?.item ?? first;
    const playLabel =
      playTarget?.season != null && playTarget.episode != null
        ? t(upNext?.resume ? 'player.resumeEpisode' : 'player.playEpisode', {
            season: playTarget.season,
            episode: playTarget.episode,
          })
        : null;
    return {
      kind: 'show',
      tmdbId: meta?.tmdbId ?? null,
      localId: show.id,
      inLibrary: true,
      title: show.title,
      year: show.year ?? null,
      rating: meta?.rating ?? null,
      overview: meta?.overview ?? null,
      tagline: meta?.tagline ?? null,
      genres: meta?.genres ?? [],
      runtimeMin: null,
      poster: c.showPosterFor(show),
      backdrop: c.backdropFor(show),
      directors: dirsFromCrew(meta?.crew),
      cast: meta?.cast ?? [],
      themeUrl: c.themeFor(show),
      video: show.video,
      playable: playTarget,
      playLabel,
      seasons: mergeSeasons(detail.seasons, discover),
      requestStatus: discover?.requestStatus ?? null,
      requestProgress: discover?.requestProgress ?? null,
      canRequest: canReq(meta?.tmdbId ?? null, discover != null),
      similar: similarShows.map((s) => ({
        key: s.id,
        title: s.title,
        poster: c.showPosterFor(s),
        genre: t('content.seasonCount', { count: s.seasonCount }),
        localId: s.id,
        tmdbId: null,
        kind: 'show' as const,
      })),
    };
  }

  // Not owned: a pure TMDB title (movie or show) driven by the discover DTO.
  const d = input.detail;
  return {
    kind: d.kind,
    tmdbId: d.tmdbId,
    localId: d.localId,
    inLibrary: d.inLibrary,
    title: d.title,
    year: d.year,
    rating: d.rating,
    overview: d.overview,
    tagline: d.tagline,
    genres: d.genres,
    runtimeMin: d.runtimeMin,
    poster: imageUrl(d.posterUrl) ?? '',
    backdrop: imageUrl(d.backdropUrl),
    directors: dirsFromCrew(d.crew),
    cast: d.cast,
    themeUrl: null,
    video: null,
    playable: null,
    playLabel: null,
    seasons: d.seasons.map((s) => ({
      number: s.season,
      name: s.name,
      episodeCount: s.episodeCount,
      episodesAvailable: s.episodesAvailable,
      available: s.available,
      requested: s.requested,
      episodes: [],
      cast: [],
    })),
    requestStatus: d.requestStatus,
    requestProgress: d.requestProgress,
    canRequest: canReq(d.tmdbId, true),
    similar: d.similar.map((e: DiscoverEntry) => ({
      key: e.inLibrary && e.localId ? e.localId : `tmdb:${e.tmdbId}`,
      title: e.title,
      poster: imageUrl(e.posterUrl) ?? '',
      genre: t(e.kind === 'show' ? 'discover.kindShow' : 'discover.kindMovie'),
      localId: e.localId,
      tmdbId: e.tmdbId,
      kind: e.kind,
    })),
  };
}

/** Merge owned seasons (real playable episodes) with the TMDB availability
 * overlay, keyed by season number. Owned-only when no discover overlay. */
function mergeSeasons(owned: Season[], discover: DiscoverDetail | null): TitleSeason[] {
  const ownedBy = new Map(owned.map((s) => [s.number, s]));
  if (!discover) {
    return owned.map((s) => ({
      number: s.number,
      name: null,
      episodeCount: s.episodes.length,
      episodesAvailable: s.episodes.length,
      available: true,
      requested: false,
      episodes: s.episodes,
      cast: s.cast ?? [],
    }));
  }
  const numbers = new Set<number>([...ownedBy.keys(), ...discover.seasons.map((s) => s.season)]);
  return [...numbers]
    .sort((a, b) => a - b)
    .map((n) => {
      const own = ownedBy.get(n);
      const ds = discover.seasons.find((s) => s.season === n);
      return {
        number: n,
        name: ds?.name ?? null,
        episodeCount: ds?.episodeCount ?? own?.episodes.length ?? 0,
        episodesAvailable: ds?.episodesAvailable ?? own?.episodes.length ?? 0,
        available: ds ? ds.available : true,
        requested: ds?.requested ?? false,
        episodes: own?.episodes ?? [],
        cast: own?.cast ?? [],
      };
    });
}

/** "2024 · 2h08" from a not-owned title's year + TMDB runtime (movies only). */
export function tmdbMetaLine(year: number | null, runtimeMin: number | null): string {
  const parts: string[] = [];
  if (year) parts.push(String(year));
  const rt = formatRuntime((runtimeMin ?? 0) * 60000);
  if (rt) parts.push(rt);
  return parts.join(' · ');
}
