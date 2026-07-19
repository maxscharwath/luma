import { compareTitles, hasGenre, isSortMode, type Sortable, type SortMode } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconCategory } from '@tabler/icons-react';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useMemo } from 'react';
import { BrowseBar } from '#web/features/catalog/browse-bar';
import { type CatalogEntry, CatalogGrid } from '#web/features/catalog/cards';
import { isAuthed } from '#web/shared/lib/api';
import { catalogQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_TITLE, SkeletonRow } from '#web/shared/ui';

interface GenreSearch {
  sort?: SortMode;
}

export const Route = createFileRoute('/_app/genre/$genre')({
  validateSearch: (s: Record<string, unknown>): GenreSearch =>
    isSortMode(s.sort) ? { sort: s.sort } : {},
  loader: async ({ context: { queryClient } }) => {
    if (!isAuthed()) return;
    await Promise.all([
      queryClient.ensureQueryData(catalogQueries.moviesView()),
      queryClient.ensureQueryData(catalogQueries.showsView()),
    ]);
  },
  pendingComponent: GenrePending,
  component: GenrePage,
});

function GenrePending() {
  const { genre } = Route.useParams();
  return (
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{genre}</h1>
      <div className="mt-6">
        <SkeletonRow count={14} />
      </div>
    </main>
  );
}

function GenrePage() {
  const t = useT();
  const { genre } = Route.useParams();
  const { sort = 'added' } = Route.useSearch();
  const navigate = Route.useNavigate();
  const { data: movies } = useSuspenseQuery(catalogQueries.moviesView());
  const { data: shows } = useSuspenseQuery(catalogQueries.showsView());

  // Every movie + show carrying this genre, mixed and ordered by the chosen sort.
  const entries = useMemo<CatalogEntry[]>(() => {
    const matched: { entry: CatalogEntry; item: Sortable }[] = [
      ...movies
        .filter((m) => hasGenre(m, genre))
        .map((m) => ({ entry: { kind: 'movie' as const, movie: m }, item: m })),
      ...shows
        .filter((s) => hasGenre(s, genre))
        .map((s) => ({ entry: { kind: 'show' as const, show: s }, item: s })),
    ];
    const cmp = compareTitles(sort);
    return [...matched].sort((a, b) => cmp(a.item, b.item)).map((x) => x.entry);
  }, [movies, shows, genre, sort]);

  return (
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{genre}</h1>
      {entries.length === 0 ? (
        <EmptyState icon={<IconCategory size={32} stroke={1.5} />} title={t('search.noResults')} />
      ) : (
        <>
          <BrowseBar
            sort={sort}
            onSort={(mode) => navigate({ search: (p) => ({ ...p, sort: mode }) })}
            genres={[]}
            onGenre={() => {}}
          />
          <CatalogGrid entries={entries} />
        </>
      )}
    </main>
  );
}
