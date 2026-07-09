// The full "Films tendance" / "Séries tendance" page: a paginated grid of this
// week's trending movies OR shows, reached via "Voir tout" on the discover
// rails. Same DiscoverCard tiles as search; TMDB-gated on requests.create.

import { hasPermission } from '@luma/core';
import { useT } from '@luma/ui';
import { IconArrowLeft, IconChevronLeft, IconChevronRight, IconFlame } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { useRef, useState } from 'react';
import { DiscoverCard } from '#web/features/requests/discover-card';
import { SkeletonRow } from '#web/features/requests/poster-skeleton';
import {
  type TrendingPageState,
  useTrendingPage,
} from '#web/features/requests/use-discover-search';
import { useAuth } from '#web/shared/lib/auth';

// Same auto-fill poster grid as the catalogue (see cards.tsx GRID).
const GRID =
  'mt-8 grid grid-cols-[repeat(auto-fill,minmax(min(var(--card-w),100%),1fr))] gap-x-4.5 gap-y-6 *:w-full!';

export function TrendingPage({ type }: Readonly<{ type: 'movie' | 'tv' }>) {
  const t = useT();
  const { user } = useAuth();
  const canDiscover = !!user && hasPermission(user, 'requests.create');
  const [page, setPage] = useState(1);
  const topRef = useRef<HTMLElement>(null);
  const state = useTrendingPage(type, page, canDiscover);
  const title = type === 'movie' ? t('discover.trendingMovies') : t('discover.trendingShows');

  // Paging is the only page-change path, so scroll back to the top right here
  // rather than in an effect that would only depend on `page` to fire.
  const go = (next: number) => {
    setPage(Math.min(Math.max(1, next), state.totalPages));
    topRef.current?.scrollIntoView({ block: 'start' });
  };

  return (
    <main ref={topRef} className="min-w-0 px-(--gutter-web) pb-20 pt-12">
      <Link
        to="/search"
        className="mb-6 inline-flex items-center gap-1.5 text-[13.5px] font-semibold text-dim transition-colors hover:text-text"
      >
        <IconArrowLeft size={16} stroke={2.2} />
        {t('discover.back')}
      </Link>

      <h1 className="flex items-center gap-2.5 font-display text-[clamp(26px,5vw,34px)] font-bold leading-tight tracking-[-.02em]">
        <IconFlame size={26} stroke={2} className="text-accent" />
        {title}
      </h1>

      {!canDiscover ? (
        <div className="mt-20 text-center text-[15px] font-medium text-dim">
          {t('discover.empty')}
        </div>
      ) : (
        <>
          <Body state={state} />
          {/* Pager persists across page changes: totalPages is retained while the
              next page loads, and the display page is optimistic (local). */}
          <Pager page={page} totalPages={state.totalPages} onGo={go} />
        </>
      )}
    </main>
  );
}

/** The grid area: skeletons while loading, an empty note when TMDB returns
 * nothing, else the poster grid. Split out so the page's own render stays a
 * single (non-nested) permission ternary. */
function Body({ state }: Readonly<{ state: TrendingPageState }>) {
  const t = useT();
  if (state.loading) {
    return (
      <div className="mt-8">
        <SkeletonRow count={12} />
      </div>
    );
  }
  if (state.entries.length === 0) {
    return (
      <div className="mt-20 text-center text-[15px] font-medium text-dim">
        {t('discover.noResults')}
      </div>
    );
  }
  return (
    <div className={GRID}>
      {state.entries.map((entry) => (
        <DiscoverCard key={`${entry.kind}-${entry.tmdbId}`} entry={entry} />
      ))}
    </div>
  );
}

function Pager({
  page,
  totalPages,
  onGo,
}: Readonly<{ page: number; totalPages: number; onGo: (n: number) => void }>) {
  const t = useT();
  if (totalPages <= 1) return null;
  const btn =
    'inline-flex h-10 items-center gap-1.5 rounded-xl border border-border-strong bg-surface-1 px-4 text-[13.5px] font-semibold text-text transition-colors hover:border-accent/60 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:border-border-strong';
  return (
    <div className="mt-10 flex items-center justify-center gap-4">
      <button type="button" className={btn} onClick={() => onGo(page - 1)} disabled={page <= 1}>
        <IconChevronLeft size={16} stroke={2.4} />
        {t('discover.prev')}
      </button>
      <span className="text-[13.5px] font-semibold tabular-nums text-dim">
        {t('discover.pageOf', { page: String(page), total: String(totalPages) })}
      </span>
      <button
        type="button"
        className={btn}
        onClick={() => onGo(page + 1)}
        disabled={page >= totalPages}
      >
        {t('discover.next')}
        <IconChevronRight size={16} stroke={2.4} />
      </button>
    </div>
  );
}
