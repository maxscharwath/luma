// The single detail page for a title, owned or not. Fed a normalized `TitleView`
// it renders one hero (Play for owned, Request/status for not-owned), the merged
// season section (play owned episodes + request missing/partial seasons), cast,
// similar, and the owned-only Treatments + AI rails. Replaces the old split
// movie/show fiche vs discover fiche.

import { apiErrorText, type EpisodeRef, formatRuntime } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconLoader2, IconPlus } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { AiSuggestRail } from '#web/features/catalog/ai-suggest-rail';
import {
  audioFlagLabel,
  audioString,
  CastRail,
  DetailHero,
  langName,
  qualityBadges,
  type SimilarItem,
  SimilarRail,
  subString,
} from '#web/features/catalog/detail';
import { SeasonSection } from '#web/features/catalog/episode-list';
import { TreatmentsPanel } from '#web/features/catalog/treatments-panel';
import { RequestStatusChip } from '#web/features/requests/request-status-chip';
import { SeasonPicker } from '#web/features/requests/season-picker';
import { useAuth } from '#web/shared/lib/auth';
import { useMyList } from '#web/shared/lib/mylist';
import { userQueries } from '#web/shared/lib/queries';
import { type TitleView, tmdbMetaLine } from '#web/shared/lib/titleView';
import { useWatched } from '#web/shared/lib/watched';

type ProgressEntry = { itemId: string; positionMs: number; durationMs?: number | null };

/** Resume-progress percentage per item id (owned show episodes). */
function progressMap(entries: readonly ProgressEntry[]): Record<string, number> {
  const map: Record<string, number> = {};
  for (const e of entries) {
    const dur = e.durationMs ?? 0;
    if (dur > 0 && e.positionMs > 0) {
      map[e.itemId] = Math.min(100, Math.round((e.positionMs / dur) * 100));
    }
  }
  return map;
}

/** Optimistically fold a fresh request into the view (top-level status + the
 * requested seasons). Per-episode asks only lift the status. */
function nextViewAfterRequest(
  v: TitleView,
  status: TitleView['requestStatus'],
  seasons: number[] | null,
  episodes?: EpisodeRef[],
): TitleView {
  if (episodes?.length) return { ...v, requestStatus: status };
  const target = new Set(
    seasons ?? v.seasons.filter((s) => !s.available && !s.requested).map((s) => s.number),
  );
  return {
    ...v,
    requestStatus: status,
    seasons: v.seasons.map((s) => (target.has(s.number) ? { ...s, requested: true } : s)),
  };
}

/** Mark the picked episodes pending (`"season-episode"` keys). */
function addPendingEpisodes(prev: Set<string>, episodes: EpisodeRef[]): Set<string> {
  const next = new Set(prev);
  for (const e of episodes) next.add(`${e.season}-${e.episode}`);
  return next;
}

/** The cinematic hero for a title (Play for owned, Request CTA otherwise). */
function TitleHero({
  view,
  owned,
  localId,
  busy,
  overline,
  isWatched,
  toggleWatched,
  inList,
  toggleList,
  onPlay,
  onRequest,
  onBack,
}: Readonly<{
  view: TitleView;
  owned: boolean;
  localId: string | null | undefined;
  busy: boolean;
  overline: string;
  isWatched: (id: string) => boolean;
  toggleWatched: (id: string) => void;
  inList: (id: string) => boolean;
  toggleList: (id: string) => void;
  onPlay: (id: string) => void;
  onRequest: () => void;
  onBack: () => void;
}>) {
  const t = useT();
  const playable = owned ? view.playable : null;
  const listState: {
    watched?: boolean;
    onToggleWatched?: () => void;
    inList?: boolean;
    onToggleList?: () => void;
  } =
    owned && localId
      ? {
          watched: isWatched(localId),
          onToggleWatched: () => toggleWatched(localId),
          inList: inList(localId),
          onToggleList: () => toggleList(localId),
        }
      : {};
  const trackInfo: { audio?: string; subtitles?: string } = playable
    ? { audio: audioString(t, playable), subtitles: subString(t, playable) }
    : {};
  return (
    <DetailHero
      art={{
        id: localId ?? String(view.tmdbId ?? view.title),
        backdrop: view.backdrop,
        poster: view.poster,
      }}
      overline={overline}
      title={view.title}
      rating={view.rating}
      meta={metaLine(t, view)}
      badges={view.video ? qualityBadges(view.video) : []}
      audioFlag={owned ? audioFlagLabel(t, view.playable) : null}
      directors={view.directors}
      tagline={view.tagline}
      overview={view.overview}
      audio={trackInfo.audio}
      subtitles={trackInfo.subtitles}
      playable={playable}
      playLabel={view.playLabel ?? undefined}
      themeUrl={view.themeUrl}
      watched={listState.watched}
      onToggleWatched={listState.onToggleWatched}
      inList={listState.inList}
      onToggleList={listState.onToggleList}
      primaryAction={
        owned ? undefined : <RequestCta view={view} busy={busy} onRequest={onRequest} />
      }
      onBack={onBack}
      onPlay={playable ? () => onPlay(playable.id) : undefined}
    />
  );
}

/** The stacked sections below the hero (treatments, cast/seasons, similar, AI). */
function TitleBody({
  view,
  owned,
  localId,
  error,
  similarItems,
  epProgress,
  busy,
  pendingEps,
  isWatched,
  toggleWatched,
  onPlay,
  onPickSeason,
  onPickAll,
  onRequestEpisode,
  onOpenSimilar,
}: Readonly<{
  view: TitleView;
  owned: boolean;
  localId: string | null | undefined;
  error: string | null;
  similarItems: SimilarItem[];
  epProgress: Record<string, number>;
  busy: boolean;
  pendingEps: Set<string>;
  isWatched: (id: string) => boolean;
  toggleWatched: (id: string) => void;
  onPlay: (id: string) => void;
  onPickSeason: (season: number) => void;
  onPickAll: () => void;
  onRequestEpisode: (season: number, episode: number) => void;
  onOpenSimilar: (key: string) => void;
}>) {
  const t = useT();
  return (
    <>
      {error ? (
        <p className="mt-2 px-(--gutter-web) text-[13.5px] font-semibold text-[#EF8091]">{error}</p>
      ) : null}

      {owned && localId ? (
        <TreatmentsPanel
          kind={view.kind === 'show' ? 'show' : 'item'}
          id={localId}
          title={view.title}
        />
      ) : null}

      {view.kind === 'movie' ? (
        <CastRail cast={view.cast} />
      ) : (
        <SeasonSection
          seasons={view.seasons}
          fallbackCast={view.cast}
          isWatched={isWatched}
          toggleWatched={toggleWatched}
          progressOf={(id) => epProgress[id] ?? null}
          onPlay={onPlay}
          canRequest={view.canRequest}
          onPickSeason={onPickSeason}
          onPickAll={onPickAll}
          onRequestEpisode={onRequestEpisode}
          pendingEpisodes={pendingEps}
          requestBusy={busy}
        />
      )}

      <SimilarRail title={t('content.similarTitles')} items={similarItems} onOpen={onOpenSimilar} />

      {owned && localId ? <AiSuggestRail id={localId} /> : null}
    </>
  );
}

export function TitleDetail({ initial }: Readonly<{ initial: TitleView }>) {
  const t = useT();
  const { client, user } = useAuth();
  const navigate = useNavigate();
  const [view, setView] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // `null` = closed; `number[]` = open (empty preselects every open season).
  const [pick, setPick] = useState<number[] | null>(null);
  // Individual episodes optimistically marked pending (`"season-episode"` keys),
  // since the view carries no per-episode request flag.
  const [pendingEps, setPendingEps] = useState<Set<string>>(() => new Set());
  const { isWatched, toggleWatched } = useWatched();
  const { inList, toggle: toggleList } = useMyList();

  const owned = view.localId != null && view.playable != null;
  const localId = view.localId;
  let backTo: '/' | '/series' | '/search' = '/search';
  if (owned) backTo = view.kind === 'show' ? '/series' : '/';

  // Per-episode resume progress for an owned show (one fetch, mapped by item id).
  const { data: epProgress = {} } = useQuery({
    ...userQueries.progress(),
    enabled: !!user && !!localId && view.kind === 'show',
    select: (entries) => progressMap(entries),
  });

  const play = (id: string) => navigate({ to: '/watch/$id', params: { id } });

  const doRequest = (seasons: number[] | null, episodes?: EpisodeRef[]) => {
    if (view.tmdbId == null) return;
    setBusy(true);
    setError(null);
    client
      .createRequest({ kind: view.kind, tmdbId: view.tmdbId, seasons, episodes })
      .then((req) => {
        setView((v) => nextViewAfterRequest(v, req.status, seasons, episodes));
        if (episodes?.length) setPendingEps((prev) => addPendingEpisodes(prev, episodes));
        setPick(null);
      })
      .catch((e) => setError(apiErrorText(e, t('discover.requestFailed'))))
      .finally(() => setBusy(false));
  };
  const requestEpisode = (season: number, episode: number) =>
    doRequest(null, [{ season, episode }]);
  // Movies request immediately; shows open the season sheet.
  const onRequestClick = () => (view.kind === 'show' ? setPick([]) : doRequest(null));

  const openSimilar = (key: string) => {
    const s = view.similar.find((x) => x.key === key);
    if (!s) return;
    if (s.localId) {
      navigate({ to: s.kind === 'show' ? '/show/$id' : '/movie/$id', params: { id: s.localId } });
    } else if (s.tmdbId != null) {
      navigate({
        to: '/discover/$type/$tmdbId',
        params: { type: s.kind === 'show' ? 'tv' : 'movie', tmdbId: String(s.tmdbId) },
      });
    }
  };

  const fallbackOverlineKey = view.kind === 'show' ? 'content.series' : 'content.film';
  const overline = view.genres.length
    ? view.genres.slice(0, 3).join(' · ')
    : t(fallbackOverlineKey);
  const similarItems: SimilarItem[] = view.similar.map((s) => ({
    id: s.key,
    title: s.title,
    genre: s.genre,
    badge: null,
    poster: s.poster,
  }));

  return (
    <main className="min-w-0 animate-[fade-in_.4s_ease] pb-20">
      <TitleHero
        view={view}
        owned={owned}
        localId={localId}
        busy={busy}
        overline={overline}
        isWatched={isWatched}
        toggleWatched={toggleWatched}
        inList={inList}
        toggleList={toggleList}
        onPlay={play}
        onRequest={onRequestClick}
        onBack={() => navigate({ to: backTo })}
      />

      <TitleBody
        view={view}
        owned={owned}
        localId={localId}
        error={error}
        similarItems={similarItems}
        epProgress={epProgress}
        busy={busy}
        pendingEps={pendingEps}
        isWatched={isWatched}
        toggleWatched={toggleWatched}
        onPlay={play}
        onPickSeason={(s) => setPick([s])}
        onPickAll={() => setPick([])}
        onRequestEpisode={requestEpisode}
        onOpenSimilar={openSimilar}
      />

      {pick !== null ? (
        <SeasonPicker
          seasons={view.seasons}
          title={view.title}
          busy={busy}
          initial={pick.length > 0 ? pick : undefined}
          onClose={() => setPick(null)}
          onRequest={doRequest}
        />
      ) : null}
    </main>
  );
}

/** The hero's primary action for a not-owned title: the live request status
 * chip once requested, else the Request button (shows open the season sheet). */
function RequestCta({
  view,
  busy,
  onRequest,
}: Readonly<{ view: TitleView; busy: boolean; onRequest: () => void }>) {
  const t = useT();
  if (view.requestStatus && view.requestStatus !== 'denied') {
    return (
      <RequestStatusChip status={view.requestStatus} size="hero" progress={view.requestProgress} />
    );
  }
  if (!view.canRequest) return null;
  return (
    <button
      type="button"
      disabled={busy}
      onClick={onRequest}
      className="inline-flex items-center gap-2 rounded-md bg-accent px-6 py-3.5 text-[15px] font-bold text-accent-ink transition-colors hover:bg-accent-hover disabled:opacity-60"
    >
      {busy ? (
        <IconLoader2 size={17} stroke={2.4} className="animate-spin" />
      ) : (
        <IconPlus size={17} stroke={2.6} />
      )}
      {view.kind === 'show' ? t('discover.requestShow') : t('discover.request')}
    </button>
  );
}

/** Hero meta line: owned movie = year · runtime · audio lang; show = year ·
 * seasons · episodes; not-owned movie = year · TMDB runtime. */
function metaLine(t: ReturnType<typeof useT>, view: TitleView): string {
  if (view.kind === 'show') {
    const episodes = view.seasons.reduce((n, s) => n + (s.episodes.length || s.episodeCount), 0);
    return [
      view.year ? String(view.year) : null,
      t('content.seasonCount', { count: view.seasons.length }),
      t('content.episodeCount', { count: episodes }),
    ]
      .filter(Boolean)
      .join(' · ');
  }
  if (view.playable) {
    return [
      view.year ? String(view.year) : null,
      formatRuntime(view.playable.durationMs),
      langName(t, view.playable.audio?.language),
    ]
      .filter(Boolean)
      .join(' · ');
  }
  return tmdbMetaLine(view.year, view.runtimeMin);
}
