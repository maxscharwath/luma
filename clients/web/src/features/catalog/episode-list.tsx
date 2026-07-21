// The unified season section: one switcher over ALL seasons; the selected
// season renders its owned, playable episodes (with watched/progress) and/or a
// request card for a missing or partial season, so a partially-owned show
// plays what it has and requests the gaps on one screen. Extracted from the old
// show fiche + the discover season cards.

import { type CastMember, formatRuntime, type MediaItem, posterColors } from '@kroma/core';
import { Image, useT } from '@kroma/ui';
import {
  IconCheck,
  IconChevronRight,
  IconFlag,
  IconPlayerPlayFilled,
  IconPlus,
} from '@tabler/icons-react';
import { type ReactNode, useState } from 'react';
import { CastRail } from '#web/features/catalog/detail';
import { ReportDialog } from '#web/features/catalog/report-dialog';
import { RequestStatusChip } from '#web/features/requests/request-status-chip';
import { kromaClient } from '#web/shared/lib/api';
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
  const still = kromaClient().backdropFor(episode);
  const num =
    episode.season != null && episode.episode != null
      ? `S${String(episode.season).padStart(2, '0')}E${String(episode.episode).padStart(2, '0')} · `
      : '';
  const reportLabel = `${num}${episode.episodeTitle ?? episode.title}`;
  return (
    <div
      className={`group flex items-center gap-3 rounded-[14px] border bg-white/2.5 p-3.5 transition-colors hover:bg-white/6 sm:gap-5 ${
        watched ? 'border-accent/30' : 'border-white/5'
      }`}
    >
      <button
        type="button"
        onClick={onPlay}
        className="flex min-w-0 flex-1 items-center gap-3 text-left focus:outline-none sm:gap-5"
      >
        <div
          className="relative flex aspect-video w-32 shrink-0 items-center justify-center overflow-hidden rounded-md sm:w-50"
          style={{ background: `linear-gradient(135deg, ${g1}, ${g2})` }}
        >
          <Image src={still} fit="cover" fill className={watched ? 'opacity-60' : ''} />
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
            <span
              className={`min-w-0 truncate text-[17px] font-bold ${watched ? 'text-white/55' : ''}`}
            >
              {episode.episode}. {episode.episodeTitle ?? episode.title}
            </span>
            {runtime ? (
              <span className="shrink-0 text-[13px] font-medium text-white/45 max-sm:text-[14px]">
                {runtime}
              </span>
            ) : null}
          </div>
          {synopsis ? (
            <p className="line-clamp-2 text-[14px] leading-normal text-white/60 max-sm:text-[15px]">
              {synopsis}
            </p>
          ) : null}
        </div>
      </button>
      <button
        type="button"
        onClick={() =>
          void ReportDialog.call({
            subjectKind: 'episode',
            subjectId: episode.id,
            subjectTitle: reportLabel,
          })
        }
        aria-label={t('report.action')}
        title={t('report.action')}
        className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-border-strong bg-white/5 text-text opacity-60 transition-colors hover:bg-white/15 hover:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100"
      >
        <IconFlag size={16} stroke={2} />
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
            : 'border-border-strong bg-white/5 text-text opacity-60 hover:bg-white/15 hover:opacity-100 group-hover:opacity-100 pointer-coarse:opacity-100'
        }`}
      >
        <IconCheck size={17} stroke={2.4} />
      </button>
    </div>
  );
}

/** A season episode not on disk: a compact row with a per-episode request button
 * (or a pending chip once asked). Rendered only when the viewer can request. */
function MissingEpisodeRow({
  episode,
  pending,
  busy,
  onRequest,
}: Readonly<{
  season: number;
  episode: number;
  pending: boolean;
  busy: boolean;
  onRequest: () => void;
}>) {
  const t = useT();
  return (
    <div className="flex items-center gap-3 rounded-[14px] border border-white/5 bg-white/1.5 p-3.5 sm:gap-5">
      <div className="flex aspect-video w-32 shrink-0 items-center justify-center rounded-md bg-white/4 text-white/35 sm:w-50">
        <span className="text-[15px] font-bold">{episode}</span>
      </div>
      <div className="min-w-0 flex-1">
        <span className="min-w-0 truncate text-[17px] font-bold text-white/70">
          {t('content.episodeN', { n: episode })}
        </span>
      </div>
      {pending ? (
        <RequestStatusChip status="pending" size="card" />
      ) : (
        <button
          type="button"
          disabled={busy}
          onClick={onRequest}
          aria-label={t('requests.requestEpisode')}
          title={t('requests.requestEpisode')}
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-accent/30 bg-accent/12 text-accent transition-colors hover:bg-accent hover:text-accent-ink disabled:opacity-50"
        >
          <IconPlus size={17} stroke={2.6} />
        </button>
      )}
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

  let tone = 'border-white/8 bg-white/3 hover:border-accent/50 hover:bg-white/6';
  if (locked) tone = 'cursor-default border-white/5 bg-white/2';
  else if (partial)
    tone = 'border-[#F4B642]/30 bg-[#F4B642]/6 hover:border-[#F4B642]/60 hover:bg-[#F4B642]/10';

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
  onRequestEpisode,
  pendingEpisodes,
  requestBusy,
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
  onRequestEpisode: (season: number, episode: number) => void;
  /** `"season-episode"` keys optimistically marked pending after a per-episode ask. */
  pendingEpisodes: Set<string>;
  requestBusy: boolean;
}>) {
  const t = useT();
  const [season, setSeason] = useState(seasons[0]?.number ?? 1);
  const current = seasons.find((s) => s.number === season) ?? seasons[0];
  if (!current) return null;
  const hasOpen = canRequest && seasons.some((s) => !s.available && !s.requested);
  const partialCurrent = canRequest && current.episodes.length > 0 && !current.available;

  // Merge owned episodes with the season's missing ones (by number) so each row
  // plays what is on disk or offers a per-episode request. Missing rows are only
  // enumerated when the viewer can request AND TMDB gave us the episode count;
  // otherwise the list stays owned-only (unchanged for no-permission viewers).
  const { ownedByNum, ordered, perEpisode } = mergeEpisodes(current, canRequest);

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

      {ordered.length > 0 ? (
        <>
          <div className="mb-5 mt-4 px-(--gutter-web) text-[14px] font-medium text-white/45">
            {t('content.episodeCount', {
              count: perEpisode ? current.episodeCount : current.episodes.length,
            })}
          </div>
          <SeasonEpisodes
            current={current}
            ordered={ordered}
            ownedByNum={ownedByNum}
            isWatched={isWatched}
            toggleWatched={toggleWatched}
            progressOf={progressOf}
            onPlay={onPlay}
            onRequestEpisode={onRequestEpisode}
            pendingEpisodes={pendingEpisodes}
            requestBusy={requestBusy}
          />
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

/** Merge the season's owned episodes with its missing ones (by number). Missing
 * rows are only enumerated when the viewer can request AND TMDB gave us the
 * episode count; otherwise the list stays owned-only. */
function mergeEpisodes(
  current: TitleSeason,
  canRequest: boolean,
): { ownedByNum: Map<number, MediaItem>; ordered: number[]; perEpisode: boolean } {
  const ownedByNum = new Map<number, MediaItem>();
  for (const ep of current.episodes) if (ep.episode != null) ownedByNum.set(ep.episode, ep);
  const perEpisode = canRequest && !current.available && current.episodeCount > 0;
  const epNumbers = new Set<number>(ownedByNum.keys());
  if (perEpisode) for (let n = 1; n <= current.episodeCount; n++) epNumbers.add(n);
  const ordered = [...epNumbers].sort((a, b) => a - b);
  return { ownedByNum, ordered, perEpisode };
}

/** The season's episode rows: each number is either an owned, playable row or a
 * per-episode request row for a gap. */
function SeasonEpisodes({
  current,
  ordered,
  ownedByNum,
  isWatched,
  toggleWatched,
  progressOf,
  onPlay,
  onRequestEpisode,
  pendingEpisodes,
  requestBusy,
}: Readonly<{
  current: TitleSeason;
  ordered: number[];
  ownedByNum: Map<number, MediaItem>;
  isWatched: (id: string) => boolean;
  toggleWatched: (id: string) => void;
  progressOf: (id: string) => number | null;
  onPlay: (id: string) => void;
  onRequestEpisode: (season: number, episode: number) => void;
  pendingEpisodes: Set<string>;
  requestBusy: boolean;
}>) {
  return (
    <div className="flex flex-col gap-3.5 px-(--gutter-web)">
      {ordered.map((n) => {
        const owned = ownedByNum.get(n);
        if (owned) {
          return (
            <EpisodeRow
              key={owned.id}
              episode={owned}
              watched={isWatched(owned.id)}
              progress={progressOf(owned.id)}
              onPlay={() => onPlay(owned.id)}
              onToggleWatched={() => toggleWatched(owned.id)}
            />
          );
        }
        return (
          <MissingEpisodeRow
            key={`m-${n}`}
            season={current.number}
            episode={n}
            pending={current.requested || pendingEpisodes.has(`${current.number}-${n}`)}
            busy={requestBusy}
            onRequest={() => onRequestEpisode(current.number, n)}
          />
        );
      })}
    </div>
  );
}
