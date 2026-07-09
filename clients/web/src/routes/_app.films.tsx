import { useT } from '@luma/ui';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { MovieGrid } from '#web/features/catalog/cards';
import { isAuthed } from '#web/shared/lib/api';
import { catalogQueries } from '#web/shared/lib/queries';
import { SkeletonRow } from '#web/shared/ui';

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
    <main className="max-w-400 px-(--gutter-web) pb-16 pt-10">
      <h2 className="mb-6 mt-2 font-display text-[28px] font-bold tracking-[-.02em]">
        {t('nav.films')}
      </h2>
      <SkeletonRow count={14} />
    </main>
  );
}

function FilmsPage() {
  const t = useT();
  const { data: movies } = useSuspenseQuery(catalogQueries.moviesView());
  return (
    <main className="max-w-400 px-(--gutter-web) pb-16 pt-10">
      <h2 className="mb-6 mt-2 font-display text-[28px] font-bold tracking-[-.02em]">
        {t('nav.films')}
      </h2>
      <MovieGrid movies={movies} />
    </main>
  );
}
