import {
  formatRuntime,
  type MediaItem,
  posterColors,
  qualityBadgeForVideo,
  type Season,
  type Translate,
} from '@luma/core';
import { useT } from '@luma/ui';
import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import { IconCheck, IconChevronDown, IconPlayerPlayFilled } from '@tabler/icons-react';
import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import {
  audioString,
  CastRail,
  DetailHero,
  qualityBadges,
  type SimilarItem,
  SimilarRail,
  subString,
} from '#web/components/detail';
import { lumaClient } from '#web/lib/api';

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

function EpisodeRow({ episode, onPlay }: Readonly<{ episode: MediaItem; onPlay: () => void }>) {
  const [g1, g2] = posterColors(episode.id);
  const runtime = formatRuntime(episode.durationMs);
  const synopsis = episode.metadata?.overview;
  return (
    <button
      type="button"
      onClick={onPlay}
      className="flex items-center gap-5 rounded-[14px] border border-white/5 bg-white/[.025] p-3.5 text-left
        transition-colors hover:bg-white/6"
    >
      <div
        className="relative flex aspect-video w-50 shrink-0 items-center justify-center overflow-hidden rounded-md"
        style={{ background: `linear-gradient(135deg, ${g1}, ${g2})` }}
      >
        <div className="absolute inset-0 bg-[linear-gradient(170deg,rgba(0,0,0,.05),rgba(0,0,0,.45))]" />
        <div className="relative flex h-11 w-11 items-center justify-center rounded-full bg-[rgba(10,10,12,.5)] backdrop-blur-xs">
          <PlayGlyph />
        </div>
      </div>
      <div className="min-w-0 flex-1">
        <div className="mb-1.5 flex items-center gap-2.5">
          <span className="text-[17px] font-bold">
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
  );
}

function Chevron() {
  return <IconChevronDown size={16} stroke={2} />;
}

function SeasonSwitcher({
  seasons,
  current,
  onPick,
}: Readonly<{ seasons: Season[]; current: number; onPick: (n: number) => void }>) {
  const t = useT();
  return (
    <DropdownMenu.Root>
      <DropdownMenu.Trigger
        className="flex items-center gap-2.5 rounded-md border border-border-strong bg-white/7 px-4.5 py-2.5
          text-[15px] font-semibold text-text outline-none transition-colors hover:bg-white/12"
      >
        {t('content.season', { number: current })}
        <Chevron />
      </DropdownMenu.Trigger>
      <DropdownMenu.Portal>
        <DropdownMenu.Content
          align="start"
          sideOffset={8}
          className="z-50 min-w-60 rounded-xl border border-border bg-[rgba(24,24,28,.97)] p-1.5 shadow-pop
            backdrop-blur-[20px] data-[state=open]:animate-[pop-in_.16s_var(--ease-out)]"
        >
          {seasons.map((s) => {
            const active = s.number === current;
            return (
              <DropdownMenu.Item
                key={s.number}
                onSelect={() => onPick(s.number)}
                className="flex cursor-pointer items-center justify-between gap-3.5 rounded-[9px] px-3.5 py-2.5
                  outline-none data-[highlighted]:bg-white/7"
              >
                <div>
                  <div
                    className={`text-[15px] font-semibold ${active ? 'text-accent' : 'text-text'}`}
                  >
                    {t('content.season', { number: s.number })}
                  </div>
                  <div className="text-[12px] font-medium text-white/40">
                    {episodesLabel(t, s.episodes.length)}
                  </div>
                </div>
                {active ? <IconCheck size={18} stroke={2.4} className="text-accent" /> : null}
              </DropdownMenu.Item>
            );
          })}
        </DropdownMenu.Content>
      </DropdownMenu.Portal>
    </DropdownMenu.Root>
  );
}

function ShowDetailPage() {
  const t = useT();
  const { detail, poster, backdrop, similar } = Route.useLoaderData();
  const navigate = useNavigate();
  const show = detail.show;
  const seasons = detail.seasons;
  const meta = show.metadata;

  const [season, setSeason] = useState(seasons[0]?.number ?? 1);
  const current = seasons.find((s) => s.number === season) ?? seasons[0];
  const firstEpisode = seasons[0]?.episodes[0] ?? null;

  const play = (id: string) => navigate({ to: '/watch/$id', params: { id } });

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
        tagline={meta?.tagline}
        overview={meta?.overview}
        audio={firstEpisode ? audioString(t, firstEpisode) : '—'}
        subtitles={firstEpisode ? subString(t, firstEpisode) : t('subtitle.none')}
        playable={firstEpisode}
        onBack={() => navigate({ to: '/series' })}
        onPlay={() => firstEpisode && play(firstEpisode.id)}
      />

      <CastRail cast={meta?.cast ?? []} />

      {current ? (
        <section className="mt-10">
          <div className="mb-2 flex flex-wrap items-center gap-3.5 px-(--gutter-web)">
            <h2 className="font-display text-[24px] font-bold tracking-[-.02em]">
              {t('content.episodes')}
            </h2>
            {seasons.length > 1 ? (
              <SeasonSwitcher seasons={seasons} current={current.number} onPick={setSeason} />
            ) : null}
          </div>
          <div className="mb-5 px-(--gutter-web) text-[14px] font-medium text-white/45">
            {episodesLabel(t, current.episodes.length)}
          </div>
          <div className="flex max-w-250 flex-col gap-3.5 px-(--gutter-web)">
            {current.episodes.map((ep) => (
              <EpisodeRow key={ep.id} episode={ep} onPlay={() => play(ep.id)} />
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
