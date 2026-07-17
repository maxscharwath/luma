import { useT } from '@kroma/ui';
import { IconMovie } from '@tabler/icons-react';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { MovieGrid } from '#web/features/catalog/cards';
import { isAuthed } from '#web/shared/lib/api';
import { catalogQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_TITLE, SkeletonRow } from '#web/shared/ui';

export const Route = createFileRoute('/_app/films')({
  loader: async ({ context: { queryClient } }) => {
    if (!isAuthed()) return;
    await queryClient.ensureQueryData(catalogQueries.moviesView());
  },
  pendingComponent: FilmsPending,
  component: FilmsPage,
});

function FilmsPending() {
  const t = useT();
  return (
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{t('nav.films')}</h1>
      <div className="mt-6">
        <SkeletonRow count={14} />
      </div>
    </main>
  );
}

function FilmsPage() {
  const t = useT();
  const { data: movies } = useSuspenseQuery(catalogQueries.moviesView());
  return (
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{t('nav.films')}</h1>
      {movies.length === 0 ? (
        <EmptyState icon={<IconMovie size={32} stroke={1.5} />} title={t('content.filmsEmpty')} />
      ) : (
        <div className="mt-6">
          <MovieGrid movies={movies} />
        </div>
      )}
    </main>
  );
}
