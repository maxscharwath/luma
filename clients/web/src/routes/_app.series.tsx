import { useT } from '@luma/ui';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { ShowGrid } from '#web/features/catalog/cards';
import { isAuthed } from '#web/shared/lib/api';
import { catalogQueries } from '#web/shared/lib/queries';
import { SkeletonRow } from '#web/shared/ui';

export const Route = createFileRoute('/_app/series')({
  loader: async ({ context: { queryClient } }) => {
    if (!isAuthed()) return;
    await queryClient.ensureQueryData(catalogQueries.showsView());
  },
  pendingComponent: SeriesPending,
  component: SeriesPage,
});

function SeriesPending() {
  const t = useT();
  return (
    <main className="max-w-400 px-(--gutter-web) pb-16 pt-10">
      <h2 className="mb-6 mt-2 font-display text-[28px] font-bold tracking-[-.02em]">
        {t('nav.series')}
      </h2>
      <SkeletonRow count={14} />
    </main>
  );
}

function SeriesPage() {
  const t = useT();
  const { data: shows } = useSuspenseQuery(catalogQueries.showsView());
  return (
    <main className="max-w-400 px-(--gutter-web) pb-16 pt-10">
      <h2 className="mb-6 mt-2 font-display text-[28px] font-bold tracking-[-.02em]">
        {t('nav.series')}
      </h2>
      <ShowGrid shows={shows} />
    </main>
  );
}
