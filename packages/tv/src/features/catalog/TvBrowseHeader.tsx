// The browse screen's chrome, split out of TvGrid: the fixed-height header that
// echoes the focused tile (section label + count, title, rating, meta line,
// quality badge) and the slim sort/genre chip strip under it. Both are
// presentational and driven entirely by props, so the screen file keeps only its
// state, its lists and the poster grid.

import {
  formatRuntime,
  type GenreCount,
  type MessageKey,
  qualityBadge,
  qualityBadgeForVideo,
  SORT_MODES,
  type SortMode,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import type { CatalogEntry } from '#tv/features/catalog/home/AmbientBackdrop';
import { badgeClasses } from '#tv/shared/TvMedia';

const SORT_LABEL_KEY: Record<SortMode, MessageKey> = {
  added: 'browse.sort.added',
  release: 'browse.sort.release',
  title: 'browse.sort.title',
  rating: 'browse.sort.rating',
};

// Compact filter chip: translucent over the ambient art, amber when active.
// rgba() literal (not a `/opacity` modifier) for the legacy webOS tier.
const CHIP_CLS =
  'shrink-0 cursor-pointer rounded-full border-none bg-[rgba(255,255,255,0.08)] px-3.5 py-1.5 font-sans text-[13px] font-semibold text-muted transition-transform focus:scale-[1.06] aria-[current=true]:bg-accent aria-[current=true]:text-accent-ink';

/** Meta line under the focused title: year · runtime|seasons · lead genres. */
function entryLine(e: CatalogEntry, seasons: string | null): string {
  const mid = e.kind === 'movie' ? formatRuntime(e.item.durationMs) : seasons;
  const genres = e.item.metadata?.genres?.slice(0, 2) ?? [];
  return [e.item.year ? String(e.item.year) : null, mid, ...genres].filter(Boolean).join(' · ');
}

/** The focused entry's quality badge (a series carries its video on the show). */
function entryBadge(e: CatalogEntry): string | null {
  return e.kind === 'movie' ? qualityBadge(e.item) : qualityBadgeForVideo(e.item.video);
}

/**
 * Fixed-height header (justify-end) so the grid never reflows as the focus echo
 * swaps titles; one truncated line keeps that guarantee.
 */
export function BrowseHeader({
  label,
  count,
  hasItems,
  focused,
}: Readonly<{
  label: string;
  count: number;
  hasItems: boolean;
  focused: CatalogEntry | null;
}>) {
  return (
    <header className="flex h-52 shrink-0 flex-col justify-end px-16">
      <div className="mb-2 font-sans text-[13px] font-bold uppercase tracking-[0.22em] text-accent">
        {label}
        {hasItems ? <span className="text-dim"> · {count}</span> : null}
      </div>
      {focused ? <FocusEcho entry={focused} /> : null}
    </header>
  );
}

/** The focused tile's title + meta line, re-keyed on every swap so the fade
 * replays. */
function FocusEcho({ entry }: Readonly<{ entry: CatalogEntry }>) {
  const t = useT();
  const rating = entry.item.metadata?.rating;
  const badge = entryBadge(entry);
  const seasons =
    entry.kind === 'show' ? t('content.seasonCount', { count: entry.item.seasonCount }) : null;
  return (
    <div key={entry.item.id} className="animate-[tv-fade-in_0.25s_ease]">
      <h1 className="m-0 max-w-240 truncate font-display text-[clamp(30px,4.8vh,46px)] font-bold leading-[1.05] tracking-[-0.02em]">
        {entry.item.title}
      </h1>
      <div className="mt-1.5 flex items-center gap-2.5 font-sans text-[15px] font-semibold text-muted">
        {rating ? <span className="font-bold text-accent">{rating.toFixed(1)}★</span> : null}
        <span>{entryLine(entry, seasons)}</span>
        {badge ? <span className={badgeClasses(badge)}>{badge}</span> : null}
      </div>
    </div>
  );
}

/** The sort + genre chip strip: every sort mode, then (when the section has any)
 * an "all genres" chip and one chip per genre. */
export function BrowseFilters({
  sort,
  onSort,
  genres,
  genre,
  onGenre,
}: Readonly<{
  sort: SortMode;
  onSort: (mode: SortMode) => void;
  genres: GenreCount[];
  genre: string | undefined;
  onGenre: (name: string | undefined) => void;
}>) {
  const t = useT();
  return (
    <div className="scrollbar-none flex shrink-0 items-center gap-2 overflow-x-auto px-16 py-3">
      {SORT_MODES.map((mode) => (
        <button
          key={mode}
          type="button"
          data-focus=""
          aria-current={mode === sort}
          onClick={() => onSort(mode)}
          className={CHIP_CLS}
        >
          {t(SORT_LABEL_KEY[mode])}
        </button>
      ))}
      {genres.length > 0 ? (
        <>
          <span className="mx-1 h-5 w-px shrink-0 bg-[rgba(255,255,255,0.14)]" />
          <button
            type="button"
            data-focus=""
            aria-current={!genre}
            onClick={() => onGenre(undefined)}
            className={CHIP_CLS}
          >
            {t('browse.allGenres')}
          </button>
          {genres.map((g) => (
            <button
              key={g.name}
              type="button"
              data-focus=""
              aria-current={g.name === genre}
              onClick={() => onGenre(g.name)}
              className={CHIP_CLS}
            >
              {g.name}
            </button>
          ))}
        </>
      ) : null}
    </div>
  );
}
