import { useT } from '@kroma/ui';
import { IconDeviceTv } from '@tabler/icons-react';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { ShowGrid } from '#web/features/catalog/cards';
import { isAuthed } from '#web/shared/lib/api';
import { catalogQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_TITLE, SkeletonRow } from '#web/shared/ui';

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
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{t('nav.series')}</h1>
      <div className="mt-6">
        <SkeletonRow count={14} />
      </div>
    </main>
  );
}

function SeriesPage() {
  const t = useT();
  const { data: shows } = useSuspenseQuery(catalogQueries.showsView());
  return (
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{t('nav.series')}</h1>
      {shows.length === 0 ? (
        <EmptyState
          icon={<IconDeviceTv size={32} stroke={1.5} />}
          title={t('content.seriesEmpty')}
        />
      ) : (
        <div className="mt-6">
          <ShowGrid shows={shows} />
        </div>
      )}
    </main>
  );
}
