// Inline "Saisons" section on the discovery detail: one card per season with
// its availability / request state, so a user can request a single season by
// clicking its card, or open the multi-select sheet via "Choose seasons".

import type { DiscoverSeason } from '@luma/core';
import { useT } from '@luma/ui';
import { IconChevronRight, IconPlus } from '@tabler/icons-react';
import { RequestStatusChip } from '#web/features/requests/RequestStatusChip';

/** `"2019-07-12"` -> `2019`; empty when the air date is missing/malformed. */
function airYear(airDate: string | null): string {
  return airDate?.slice(0, 4) ?? '';
}

export function DiscoverSeasons({
  seasons,
  onPickAll,
  onPickOne,
}: Readonly<{
  seasons: DiscoverSeason[];
  /** Open the multi-select sheet (every open season preselected). */
  onPickAll: () => void;
  /** Request one specific season (opens the sheet with just it ticked). */
  onPickOne: (season: number) => void;
}>) {
  const t = useT();
  if (seasons.length === 0) return null;
  const hasOpen = seasons.some((s) => !s.available && !s.requested);

  return (
    <section className="mt-10 px-(--gutter-web)">
      <div className="mb-4.5 flex items-center justify-between">
        <h2 className="font-display text-[22px] font-bold tracking-[-.02em]">
          {t('discover.seasonsTitle')}
        </h2>
        {hasOpen ? (
          <button
            type="button"
            onClick={onPickAll}
            className="inline-flex items-center gap-1.5 rounded-full border border-white/12 bg-white/6 px-3.5 py-1.5 text-[12.5px] font-semibold text-white/80 transition-colors hover:bg-white/12 hover:text-white"
          >
            {t('discover.requestSeasons')}
            <IconChevronRight size={15} stroke={2.2} />
          </button>
        ) : null}
      </div>

      <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-2.5">
        {seasons.map((s) => (
          <SeasonCard key={s.season} s={s} onPick={() => onPickOne(s.season)} />
        ))}
      </div>
    </section>
  );
}

function SeasonCard({ s, onPick }: Readonly<{ s: DiscoverSeason; onPick: () => void }>) {
  const t = useT();
  const locked = s.available || s.requested;
  // Some but not all episodes on disk: still requestable (fills only the gaps).
  const partial = !s.available && s.episodesAvailable > 0;
  const year = airYear(s.airDate);
  const epLabel = partial
    ? t('discover.episodesPartial', {
        have: String(s.episodesAvailable),
        total: String(s.episodeCount),
      })
    : t('discover.episodesN', { n: String(s.episodeCount) });
  const sub = [epLabel, year].filter(Boolean).join(' · ');

  return (
    <button
      type="button"
      disabled={locked}
      onClick={onPick}
      title={partial ? t('discover.fillGapsHint') : undefined}
      className={`group flex items-center gap-3 rounded-xl border px-4 py-3 text-left transition-colors ${
        locked
          ? 'cursor-default border-white/[0.05] bg-white/[0.02]'
          : partial
            ? 'border-[#F4B642]/30 bg-[#F4B642]/[0.06] hover:border-[#F4B642]/60 hover:bg-[#F4B642]/[0.10]'
            : 'border-white/[0.08] bg-white/[0.03] hover:border-accent/50 hover:bg-white/[0.06]'
      }`}
    >
      <span className="min-w-0 flex-1">
        <span className="block truncate text-[14px] font-bold">
          {s.name ?? t('discover.seasonN', { n: String(s.season) })}
        </span>
        <span
          className={`mt-0.5 block truncate text-[12px] font-medium ${partial ? 'text-[#F4B642]' : 'text-white/45'}`}
        >
          {sub}
        </span>
      </span>
      {locked ? (
        <RequestStatusChip status={s.available ? 'available' : 'pending'} size="card" />
      ) : (
        <span
          className={`flex h-7 w-7 shrink-0 items-center justify-center rounded-full transition-colors ${
            partial
              ? 'bg-[#F4B642]/15 text-[#F4B642] group-hover:bg-[#F4B642] group-hover:text-black'
              : 'bg-accent/12 text-accent group-hover:bg-accent group-hover:text-accent-ink'
          }`}
        >
          <IconPlus size={16} stroke={2.6} />
        </span>
      )}
    </button>
  );
}
