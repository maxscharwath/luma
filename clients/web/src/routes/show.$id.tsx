import {
  formatRuntime,
  type MediaItem,
  posterColors,
  qualityBadgeForVideo,
  type Season,
  type Translate,
  type UpNext,
} from '@luma/core';
import { useT } from '@luma/ui';
import { IconCheck, IconPlayerPlayFilled } from '@tabler/icons-react';
import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import {
  audioString,
  CastRail,
  DetailHero,
  directorsOf,
  qualityBadges,
  type SimilarItem,
  SimilarRail,
  subString,
} from '#web/features/catalog/detail';
import { lumaClient } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { useMyList } from '#web/shared/lib/mylist';
import { useWatched } from '#web/shared/lib/watched';

export const Route = createFileRoute('/show/$id')({
  loader: async ({ params }) => {
    const c = lumaClient();
    const [detail, shows] = await Promise.all([c.show(params.id), c.shows()]);
    const show = detail.show;
    const genres = new Set(show.metadata?.genres ?? []);
    const others = shows.filter((s) => s.id !== show.id);
    const related = others.filter((s) => (s.metadata?.genres ?? []).some((g) => genres.has(g)));
    const pool = (related.length >= 3 ? related : others).slice(0, 12);
    const similar: SimilarItem[] = pool.map((s) => ({
      id: s.id,
      title: s.title,
      // Season count localized at render via `seasonCount` (carry the raw count).
      genre: '',
      seasonCount: s.seasonCount,
      badge: qualityBadgeForVideo(s.video),
      poster: c.showPosterFor(s),
    }));
    return {
      detail,
      poster: c.showPosterFor(show),
      backdrop: c.backdropFor(show),
      themeUrl: c.themeFor(show),
      similar,
    };
  },
  component: ShowDetailPage,
});

/** Localized "N saison(s)" / "N épisode(s)" line (plural via the catalog). */
function seasonsLabel(t: Translate, n: number): string {
  return t('content.seasonCount', { count: n });
}
function episodesLabel(t: Translate, n: number): string {
  return t('content.episodeCount', { count: n });
}

function PlayGlyph() {
  return <IconPlayerPlayFilled size={18} color="#fff" />;
}

function EpisodeRow({
  episode,
  watched,
  progress,
  onPlay,
  onToggleWatched,
}: Readonly<{
  episode: MediaItem;
  watched: boolean;
  /** Resume progress (%) for this episode, or null. */
  progress: number | null;
  onPlay: () => void;
  onToggleWatched: () => void;
}>) {
  const t = useT();
  const [g1, g2] = posterColors(episode.id);
  const runtime = formatRuntime(episode.durationMs);
  const synopsis = episode.metadata?.overview;
  // Per-episode still (TMDB), resolved to the local WebP cache; gradient fallback.
  const still = lumaClient().backdropFor(episode);
  const [imgOk, setImgOk] = useState(true);
  const showImg = Boolean(still) && imgOk;
  return (
    <div
      className={`group flex items-center gap-5 rounded-[14px] border bg-white/[.025] p-3.5 transition-colors hover:bg-white/6 ${
        watched ? 'border-accent/30' : 'border-white/5'
      }`}
    >
      <button
        type="button"
        onClick={onPlay}
        className="flex min-w-0 flex-1 items-center gap-5 text-left focus:outline-none"
      >
        <div
          className="relative flex aspect-video w-50 shrink-0 items-center justify-center overflow-hidden rounded-md"
          style={{ background: `linear-gradient(135deg, ${g1}, ${g2})` }}
        >
          {showImg ? (
            <img
              src={still ?? undefined}
              alt=""
              loading="lazy"
              decoding="async"
              draggable={false}
              onError={() => setImgOk(false)}
              className={`absolute inset-0 h-full w-full object-cover ${watched ? 'opacity-60' : ''}`}
            />
          ) : null}
          <div className="absolute inset-0 bg-[linear-gradient(170deg,rgba(0,0,0,.05),rgba(0,0,0,.45))]" />
          {watched ? (
            <div className="absolute left-2 top-2 flex h-6 w-6 items-center justify-center rounded-full bg-accent text-black shadow-card">
              <IconCheck size={14} stroke={3} />
            </div>
          ) : null}
          <div className="relative flex h-11 w-11 items-center justify-center rounded-full bg-[rgba(10,10,12,.5)] backdrop-blur-xs transition-transform group-hover:scale-110">
            <PlayGlyph />
          </div>
          {progress != null && !watched ? (
            <div className="absolute inset-x-0 bottom-0 h-1 bg-white/25">
              <div className="h-full bg-accent" style={{ width: `${progress}%` }} />
            </div>
          ) : null}
        </div>
        <div className="min-w-0 flex-1">
          <div className="mb-1.5 flex items-center gap-2.5">
            <span className={`text-[17px] font-bold ${watched ? 'text-white/55' : ''}`}>
              {episode.episode}. {episode.episodeTitle ?? episode.title}
            </span>
            {runtime ? (
              <span className="text-[13px] font-medium text-white/45">{runtime}</span>
            ) : null}
          </div>
          {synopsis ? (
            <p className="line-clamp-2 text-[14px] leading-[1.5] text-white/60">{synopsis}</p>
          ) : null}
        </div>
      </button>
      <button
        type="button"
        onClick={onToggleWatched}
        aria-pressed={watched}
        aria-label={watched ? t('content.markUnwatched') : t('content.markWatched')}
        title={watched ? t('content.watched') : t('content.markWatched')}
        className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-full border transition-colors ${
          watched
            ? 'border-accent bg-accent text-black'
            : 'border-border-strong bg-white/5 text-text opacity-60 hover:bg-white/15 hover:opacity-100 group-hover:opacity-100'
        }`}
      >
        <IconCheck size={17} stroke={2.4} />
      </button>
    </div>
  );
}

/** Season selector a horizontally-scrollable pill row (no portal/Radix, so it
 * can't be clipped or mispositioned). The active season is filled amber. */
function SeasonSwitcher({
  seasons,
  current,
  onPick,
}: Readonly<{ seasons: Season[]; current: number; onPick: (n: number) => void }>) {
  const t = useT();
  return (
    <div className="scrollbar-none flex gap-2 overflow-x-auto px-(--gutter-web)">
      {seasons.map((s) => {
        const active = s.number === current;
        return (
          <button
            key={s.number}
            type="button"
            onClick={() => onPick(s.number)}
            aria-current={active}
            className={`shrink-0 rounded-full px-4.5 py-2 text-[14px] font-semibold transition-colors ${
              active
                ? 'bg-accent text-black'
                : 'border border-border-strong bg-white/7 text-text hover:bg-white/12'
            }`}
          >
            {t('content.season', { number: s.number })}
          </button>
        );
      })}
    </div>
  );
}

function ShowDetailPage() {
  const t = useT();
  const { detail, poster, backdrop, themeUrl, similar } = Route.useLoaderData();
  const navigate = useNavigate();
  const { isWatched, toggleWatched } = useWatched();
  const { inList, toggle: toggleList } = useMyList();
  const { client, user } = useAuth();
  const show = detail.show;
  const seasons = detail.seasons;
  const meta = show.metadata;

  const [season, setSeason] = useState(seasons[0]?.number ?? 1);
  const current = seasons.find((s) => s.number === season) ?? seasons[0];
  const firstEpisode = seasons[0]?.episodes[0] ?? null;

  // "Continue the series": resume the in-progress episode, else the next unwatched
  // (per-user, server-computed). Falls back to the first episode while loading.
  const [upNext, setUpNext] = useState<UpNext | null>(null);
  useEffect(() => {
    if (!user) return;
    let cancelled = false;
    client
      .upNext(show.id)
      .then((r) => {
        if (!cancelled) setUpNext(r);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, user, show.id]);

  const playTarget = upNext?.item ?? firstEpisode;
  const playLabel =
    playTarget?.season != null && playTarget.episode != null
      ? t(upNext?.resume ? 'player.resumeEpisode' : 'player.playEpisode', {
          season: playTarget.season,
          episode: playTarget.episode,
        })
      : undefined;

  const play = (id: string) => navigate({ to: '/watch/$id', params: { id } });

  // Per-episode resume progress (one fetch for this detail page, mapped by item id).
  const [epProgress, setEpProgress] = useState<Record<string, number>>({});
  useEffect(() => {
    if (!user) return;
    let cancelled = false;
    client
      .progress()
      .then((entries) => {
        if (cancelled) return;
        const map: Record<string, number> = {};
        for (const e of entries) {
          const dur = e.durationMs ?? 0;
          if (dur > 0 && e.positionMs > 0) {
            map[e.itemId] = Math.min(100, Math.round((e.positionMs / dur) * 100));
          }
        }
        setEpProgress(map);
      })
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, user]);

  const metaParts = [
    show.year ? String(show.year) : null,
    seasonsLabel(t, show.seasonCount),
    episodesLabel(t, show.episodeCount),
  ].filter(Boolean);

  return (
    <main className="animate-[fade-in_.4s_ease] pb-16">
      <DetailHero
        art={{ id: show.id, backdrop, poster }}
        overline={t('content.seriesOverline', { seasons: seasonsLabel(t, show.seasonCount) })}
        title={show.title}
        rating={meta?.rating}
        meta={metaParts.join(' · ')}
        badges={qualityBadges(show.video)}
        directors={directorsOf(meta)}
        tagline={meta?.tagline}
        overview={meta?.overview}
        audio={firstEpisode ? audioString(t, firstEpisode) : '-'}
        subtitles={firstEpisode ? subString(t, firstEpisode) : t('subtitle.none')}
        playable={playTarget ?? firstEpisode}
        playLabel={playLabel}
        themeUrl={themeUrl}
        watched={isWatched(show.id)}
        onToggleWatched={() => toggleWatched(show.id)}
        inList={inList(show.id)}
        onToggleList={() => toggleList(show.id)}
        onBack={() => navigate({ to: '/series' })}
        onPlay={() => playTarget && play(playTarget.id)}
      />

      {current ? (
        <section className="mt-10">
          <h2 className="mb-4 px-(--gutter-web) font-display text-[24px] font-bold tracking-[-.02em]">
            {t('content.episodes')}
          </h2>
          {seasons.length > 1 ? (
            <div className="mb-2">
              <SeasonSwitcher seasons={seasons} current={current.number} onPick={setSeason} />
            </div>
          ) : null}
          {/* Cast for the selected season (TMDB season credits), falling back to
              the show's overall cast when the season's isn't resolved yet. */}
          <CastRail cast={current.cast?.length ? current.cast : (meta?.cast ?? [])} />
          <div className="mb-5 mt-4 px-(--gutter-web) text-[14px] font-medium text-white/45">
            {episodesLabel(t, current.episodes.length)}
          </div>
          <div className="flex flex-col gap-3.5 px-(--gutter-web)">
            {current.episodes.map((ep) => (
              <EpisodeRow
                key={ep.id}
                episode={ep}
                watched={isWatched(ep.id)}
                progress={epProgress[ep.id] ?? null}
                onPlay={() => play(ep.id)}
                onToggleWatched={() => toggleWatched(ep.id)}
              />
            ))}
          </div>
        </section>
      ) : null}

      <SimilarRail
        title={t('content.similarShows')}
        items={similar.map((s) => ({
          ...s,
          genre: seasonsLabel(t, s.seasonCount ?? 0),
        }))}
        onOpen={(id) => navigate({ to: '/show/$id', params: { id } })}
      />
    </main>
  );
}
