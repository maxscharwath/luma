// One "Manquants" group card: the title header (poster, name, missing count OR
// the movie's release line) and, for a series, its missing-episode rows. Long
// episode lists collapse behind a "show more" toggle so one gappy series can't
// swallow the page. Row/series checkboxes and search buttons report back to the
// page through callbacks; all mutation state lives in `missing.tsx`.

import { type CalendarEntry, episodeTag, posterColors, sizedImageUrl } from '@kroma/core';
import { useLocale, useT } from '@kroma/ui';
import { IconLoader2, IconSearch } from '@tabler/icons-react';
import { useState } from 'react';
import { relativeAirDate } from '#web/features/requests/airdate';

export interface MissingGroup {
  /** The parent request, or null for a library-scan gap (never requested). */
  requestId: string | null;
  tmdbId: number;
  kind: CalendarEntry['kind'];
  title: string;
  year: number | null;
  posterUrl: string | null;
  items: CalendarEntry[];
}

/** A stable key for one missing row (unique across the whole list). */
export function epKey(e: CalendarEntry): string {
  const groupKey = e.requestId ?? `tmdb:${e.tmdbId}`;
  return `${groupKey}:${e.season ?? 0}:${e.episode ?? 0}`;
}

/** Episode lists longer than this collapse behind a "show more" toggle. */
const COLLAPSE_OVER = 12;
/** How many rows a collapsed list keeps visible. */
const COLLAPSED_ROWS = 10;

export function MissingGroupCard({
  group,
  canManage,
  busyKeys,
  selected,
  onToggleRow,
  onToggleGroup,
  onSearch,
  onOpen,
}: Readonly<{
  group: MissingGroup;
  canManage: boolean;
  busyKeys: Set<string>;
  selected: Set<string>;
  onToggleRow: (key: string) => void;
  onToggleGroup: (pick: boolean) => void;
  onSearch: (items: CalendarEntry[]) => void;
  onOpen: () => void;
}>) {
  const t = useT();
  const locale = useLocale();
  const [expanded, setExpanded] = useState(false);
  const [c1, c2] = posterColors(String(group.tmdbId));
  const poster = sizedImageUrl(group.posterUrl, 92);

  const isMovie = group.kind === 'movie';
  const episodes = isMovie ? [] : group.items.filter((i) => i.season != null && i.episode != null);
  const collapsed = !expanded && episodes.length > COLLAPSE_OVER;
  const visible = collapsed ? episodes.slice(0, COLLAPSED_ROWS) : episodes;

  const groupBusy = group.items.some((i) => busyKeys.has(epKey(i)));
  const allPicked = group.items.length > 0 && group.items.every((e) => selected.has(epKey(e)));
  // A gap is actionable by any requester; a request needs manage.
  const canAct = group.requestId ? canManage : true;

  // Movies carry their release info on the header line (they have no rows).
  const movieRel = isMovie ? relativeAirDate(group.items[0]?.airDate ?? null, locale) : '';

  return (
    <section className="overflow-hidden rounded-2xl border border-border bg-surface-1">
      <div className="flex items-center gap-3.5 border-b border-white/[0.06] p-3.5 last:border-b-0">
        {canAct ? (
          <Check on={allPicked} onClick={() => onToggleGroup(!allPicked)} />
        ) : (
          <span className="w-[18px]" />
        )}
        <button
          type="button"
          onClick={onOpen}
          className="group/head flex min-w-0 flex-1 items-center gap-3.5 text-left"
        >
          <div
            className="h-[52px] w-[36px] flex-[0_0_36px] overflow-hidden rounded-md"
            style={{ background: `linear-gradient(158deg, ${c1}, ${c2})` }}
          >
            {poster ? <img src={poster} alt="" className="h-full w-full object-cover" /> : null}
          </div>
          <div className="min-w-0">
            <div className="truncate text-[15px] font-bold transition-colors group-hover/head:text-accent">
              {group.title}
            </div>
            <div className="mt-0.5 truncate text-[12.5px] font-semibold text-dim">
              {isMovie ? (
                <>
                  <span className="text-[#EFB661]">{t('requests.missingMovie')}</span>
                  {group.year ? <span> · {group.year}</span> : null}
                  {movieRel ? <span> · {movieRel}</span> : null}
                </>
              ) : (
                <span className="text-[#EFB661]">
                  {t('requests.missingCount', { count: episodes.length })}
                </span>
              )}
            </div>
          </div>
        </button>
        {canAct ? (
          <button
            type="button"
            disabled={groupBusy}
            onClick={() => onSearch(group.items)}
            title={t('requests.searchTitle')}
            className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3 text-[12.5px] font-bold text-white/75 hover:text-accent disabled:opacity-50"
          >
            {groupBusy ? (
              <IconLoader2 size={15} stroke={2.4} className="animate-spin" />
            ) : (
              <IconSearch size={15} stroke={2.2} />
            )}
            {t('requests.search')}
          </button>
        ) : null}
      </div>
      {episodes.length > 0 ? (
        <ul className="divide-y divide-white/[0.04]">
          {visible.map((e) => (
            <EpisodeRow
              key={epKey(e)}
              entry={e}
              canAct={canAct}
              busy={busyKeys.has(epKey(e))}
              picked={selected.has(epKey(e))}
              onToggle={() => onToggleRow(epKey(e))}
              onSearch={() => onSearch([e])}
            />
          ))}
          {episodes.length > COLLAPSE_OVER ? (
            <li>
              <button
                type="button"
                onClick={() => setExpanded((v) => !v)}
                className="w-full px-3.5 py-2.5 text-left text-[12.5px] font-bold text-dim hover:text-accent"
              >
                {collapsed
                  ? t('requests.showMore', { count: episodes.length - COLLAPSED_ROWS })
                  : t('requests.showLess')}
              </button>
            </li>
          ) : null}
        </ul>
      ) : null}
    </section>
  );
}

function EpisodeRow({
  entry,
  canAct,
  busy,
  picked,
  onToggle,
  onSearch,
}: Readonly<{
  entry: CalendarEntry;
  canAct: boolean;
  busy: boolean;
  picked: boolean;
  onToggle: () => void;
  onSearch: () => void;
}>) {
  const t = useT();
  const locale = useLocale();
  const rel = relativeAirDate(entry.airDate, locale);

  return (
    <li className="flex items-center gap-3.5 px-3.5 py-2.5 transition-colors hover:bg-white/[0.03]">
      {canAct ? <Check on={picked} onClick={onToggle} /> : <span className="w-[18px]" />}
      <span className="w-[62px] flex-[0_0_62px] font-mono text-[13px] font-bold text-accent tabular-nums">
        {episodeTag(entry)}
      </span>
      <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-dim">
        {rel ? (
          <span className="inline-block first-letter:uppercase">{rel}</span>
        ) : (
          <span className="italic text-white/35">{t('requests.noDate')}</span>
        )}
      </span>
      {canAct ? (
        <button
          type="button"
          disabled={busy}
          onClick={onSearch}
          title={t('requests.searchTitle')}
          className="flex h-8 w-8 items-center justify-center rounded-lg text-white/45 hover:bg-white/5 hover:text-accent disabled:opacity-40"
        >
          {busy ? (
            <IconLoader2 size={15} stroke={2.2} className="animate-spin" />
          ) : (
            <IconSearch size={15} stroke={2} />
          )}
        </button>
      ) : null}
    </li>
  );
}

function Check({ on, onClick }: Readonly<{ on: boolean; onClick: () => void }>) {
  const t = useT();
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      aria-label={t('requests.select')}
      className={`flex h-[18px] w-[18px] flex-[0_0_18px] items-center justify-center rounded-[5px] border transition-colors ${
        on ? 'border-accent bg-accent text-accent-ink' : 'border-white/25 hover:border-white/50'
      }`}
    >
      {on ? (
        <svg viewBox="0 0 12 12" width="11" height="11" fill="none" aria-hidden="true">
          <path
            d="M2 6.2 4.6 9 10 3"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      ) : null}
    </button>
  );
}
