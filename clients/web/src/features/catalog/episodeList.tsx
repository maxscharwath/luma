// The unified season section: one switcher over ALL seasons; the selected
// season renders its owned, playable episodes (with watched/progress) and/or a
// request card for a missing or partial season — so a partially-owned show
// plays what it has and requests the gaps on one screen. Extracted from the old
// show fiche + the discover season cards.

import { type CastMember, formatRuntime, type MediaItem, posterColors } from '@luma/core';
import { useT } from '@luma/ui';
import { IconCheck, IconChevronRight, IconPlayerPlayFilled, IconPlus } from '@tabler/icons-react';
import { type ReactNode, useState } from 'react';
import { CastRail } from '#web/features/catalog/detail';
import { RequestStatusChip } from '#web/features/requests/RequestStatusChip';
import { lumaClient } from '#web/shared/lib/api';
import type { TitleSeason } from '#web/shared/lib/titleView';

function EpisodeRow({
  episode,
  watched,
  progress,
  onPlay,
  onToggleWatched,
}: Readonly<{
  episode: MediaItem;
  progress: number | null;
  watched: boolean;
  onPlay: () => void;
  onToggleWatched: () => void;
}>) {
  const t = useT();
  const [g1, g2] = posterColors(episode.id);
  const runtime = formatRuntime(episode.durationMs);
  const synopsis = episode.metadata?.overview;
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
            <IconPlayerPlayFilled size={18} color="#fff" />
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

/** Horizontally-scrollable season pill row; the active season is filled amber. */
function SeasonSwitcher({
  seasons,
  current,
  onPick,
}: Readonly<{ seasons: TitleSeason[]; current: number; onPick: (n: number) => void }>) {
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

/** A not-owned or partial season: request the whole season (or fill the gaps),
 * or show its availability/request chip. */
function SeasonRequestCard({
  s,
  canRequest,
  onPick,
}: Readonly<{ s: TitleSeason; canRequest: boolean; onPick: () => void }>) {
  const t = useT();
  const partial = !s.available && s.episodesAvailable > 0;
  const locked = s.available || s.requested || !canRequest;
  const epLabel = partial
    ? t('discover.episodesPartial', {
        have: String(s.episodesAvailable),
        total: String(s.episodeCount),
      })
    : t('discover.episodesN', { n: String(s.episodeCount) });

  let tone = 'border-white/[0.08] bg-white/[0.03] hover:border-accent/50 hover:bg-white/[0.06]';
  if (locked) tone = 'cursor-default border-white/[0.05] bg-white/[0.02]';
  else if (partial)
    tone =
      'border-[#F4B642]/30 bg-[#F4B642]/[0.06] hover:border-[#F4B642]/60 hover:bg-[#F4B642]/[0.10]';

  let trailing: ReactNode = null;
  if (s.available || s.requested) {
    trailing = <RequestStatusChip status={s.available ? 'available' : 'pending'} size="card" />;
  } else if (canRequest) {
    trailing = (
      <span
        className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full transition-colors ${
          partial
            ? 'bg-[#F4B642]/15 text-[#F4B642] group-hover:bg-[#F4B642] group-hover:text-black'
            : 'bg-accent/12 text-accent group-hover:bg-accent group-hover:text-accent-ink'
        }`}
      >
        <IconPlus size={16} stroke={2.6} />
      </span>
    );
  }

  return (
    <div className="px-(--gutter-web)">
      <button
        type="button"
        disabled={locked}
        onClick={onPick}
        title={partial ? t('discover.fillGapsHint') : undefined}
        className={`group flex w-full max-w-md items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors ${tone}`}
      >
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[14px] font-bold">
            {s.name ?? t('discover.seasonN', { n: String(s.number) })}
          </span>
          <span
            className={`mt-0.5 block truncate text-[12px] font-medium ${partial ? 'text-[#F4B642]' : 'text-white/45'}`}
          >
            {epLabel}
          </span>
        </span>
        {trailing}
      </button>
    </div>
  );
}

export function SeasonSection({
  seasons,
  fallbackCast,
  isWatched,
  toggleWatched,
  progressOf,
  onPlay,
  canRequest,
  onPickSeason,
  onPickAll,
}: Readonly<{
  seasons: TitleSeason[];
  fallbackCast: CastMember[];
  isWatched: (id: string) => boolean;
  toggleWatched: (id: string) => void;
  progressOf: (id: string) => number | null;
  onPlay: (id: string) => void;
  canRequest: boolean;
  onPickSeason: (season: number) => void;
  onPickAll: () => void;
}>) {
  const t = useT();
  const [season, setSeason] = useState(seasons[0]?.number ?? 1);
  const current = seasons.find((s) => s.number === season) ?? seasons[0];
  if (!current) return null;
  const hasOpen = canRequest && seasons.some((s) => !s.available && !s.requested);
  const partialCurrent = canRequest && current.episodes.length > 0 && !current.available;

  return (
    <section className="mt-10">
      <div className="mb-4 flex items-center justify-between gap-3 px-(--gutter-web)">
        <h2 className="font-display text-[24px] font-bold tracking-[-.02em]">
          {t('content.episodes')}
        </h2>
        {hasOpen ? (
          <button
            type="button"
            onClick={onPickAll}
            title={t('discover.requestSeasonsHint')}
            className="inline-flex shrink-0 items-center gap-1.5 rounded-full border border-accent/30 bg-accent/10 px-3.5 py-1.5 text-[12.5px] font-bold text-accent transition-colors hover:bg-accent/20"
          >
            <IconPlus size={14} stroke={2.6} />
            {t('discover.requestSeasons')}
            <IconChevronRight size={14} stroke={2.4} />
          </button>
        ) : null}
      </div>

      {seasons.length > 1 ? (
        <div className="mb-2">
          <SeasonSwitcher seasons={seasons} current={current.number} onPick={setSeason} />
        </div>
      ) : null}

      <CastRail cast={current.cast.length ? current.cast : fallbackCast} />

      {current.episodes.length > 0 ? (
        <>
          <div className="mb-5 mt-4 px-(--gutter-web) text-[14px] font-medium text-white/45">
            {t('content.episodeCount', { count: current.episodes.length })}
          </div>
          <div className="flex flex-col gap-3.5 px-(--gutter-web)">
            {current.episodes.map((ep) => (
              <EpisodeRow
                key={ep.id}
                episode={ep}
                watched={isWatched(ep.id)}
                progress={progressOf(ep.id)}
                onPlay={() => onPlay(ep.id)}
                onToggleWatched={() => toggleWatched(ep.id)}
              />
            ))}
          </div>
          {partialCurrent ? (
            <div className="mt-3.5">
              <SeasonRequestCard
                s={current}
                canRequest={canRequest}
                onPick={() => onPickSeason(current.number)}
              />
            </div>
          ) : null}
        </>
      ) : (
        <div className="mt-4">
          <SeasonRequestCard
            s={current}
            canRequest={canRequest}
            onPick={() => onPickSeason(current.number)}
          />
        </div>
      )}
    </section>
  );
}
