// The browse-first empty state: trending movies + shows as home-style rails,
// so the discovery page is a place to browse, not just a search box. Filtered
// by the active type chip.

import type { DiscoverEntry, DiscoverType } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconChevronRight, IconFlame } from '@tabler/icons-react';
import { Link } from '@tanstack/react-router';
import { DiscoverCard } from '#web/features/requests/discover-card';
import { Rail, SkeletonRow } from '#web/shared/ui';

const RAIL_HEADING =
  'flex items-center gap-2 font-display text-[22px] font-bold tracking-[-.02em] text-text';
const SECTION_TITLE = `mb-4 mt-9 ${RAIL_HEADING}`;

function TrendRail({
  title,
  entries,
  linkType,
}: Readonly<{ title: string; entries: DiscoverEntry[]; linkType: 'movie' | 'tv' }>) {
  const t = useT();
  if (entries.length === 0) return null;
  return (
    <section>
      <div className="mb-4 mt-9 flex items-center justify-between gap-3">
        <h2 className={RAIL_HEADING}>
          <IconFlame size={20} stroke={2} className="text-accent" />
          {title}
        </h2>
        <Link
          to="/trending/$type"
          params={{ type: linkType }}
          className="inline-flex shrink-0 items-center gap-1 text-[13px] font-semibold text-dim transition-colors hover:text-accent"
        >
          {t('discover.seeAll')}
          <IconChevronRight size={15} stroke={2.4} />
        </Link>
      </div>
      <Rail label={title}>
        {entries.map((e) => (
          <DiscoverCard key={`${e.kind}-${e.tmdbId}`} entry={e} />
        ))}
      </Rail>
    </section>
  );
}

export function TrendingBrowse({
  entries,
  loading,
  type,
}: Readonly<{ entries: DiscoverEntry[]; loading: boolean; type: DiscoverType }>) {
  const t = useT();

  if (loading) {
    return (
      <div className="mt-9">
        <h2 className={SECTION_TITLE}>
          <IconFlame size={20} stroke={2} className="text-accent" />
          {t('discover.trending')}
        </h2>
        <SkeletonRow />
      </div>
    );
  }

  const movies = entries.filter((e) => e.kind === 'movie');
  const shows = entries.filter((e) => e.kind === 'show');
  const wantMovies = type !== 'tv';
  const wantShows = type !== 'movie';

  return (
    <div className="animate-[fade-in_.3s_var(--ease-out)]">
      {wantMovies ? (
        <TrendRail title={t('discover.trendingMovies')} entries={movies} linkType="movie" />
      ) : null}
      {wantShows ? (
        <TrendRail title={t('discover.trendingShows')} entries={shows} linkType="tv" />
      ) : null}
    </div>
  );
}
