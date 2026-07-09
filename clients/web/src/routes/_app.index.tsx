import { useT } from '@luma/ui';
import { IconMovie } from '@tabler/icons-react';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { Hero, ShowRail } from '#web/features/catalog/cards';
import { ContinueRow } from '#web/features/catalog/continue-row';
import { HomeSections } from '#web/features/catalog/home-sections';
import { isAuthed } from '#web/shared/lib/api';
import { catalogQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PageSkeleton } from '#web/shared/ui';

export const Route = createFileRoute('/_app/')({
  loader: async ({ context: { queryClient } }) => {
    // The catalogue is auth-gated: skip until signed in (the gate covers the UI;
    // the root invalidates queries on login so these prefetch then). Prefetch
    // into the shared cache so the component's useSuspenseQuery reads it warm.
    if (!isAuthed()) return;
    await Promise.all([
      queryClient.ensureQueryData(catalogQueries.moviesView()),
      queryClient.ensureQueryData(catalogQueries.showsView()),
    ]);
  },
  pendingComponent: () => <PageSkeleton rails={3} />,
  component: HomePage,
});

function HomePage() {
  const t = useT();
  const { data: movies } = useSuspenseQuery(catalogQueries.moviesView());
  const { data: shows } = useSuspenseQuery(catalogQueries.showsView());
  if (movies.length === 0 && shows.length === 0) {
    return (
      <main className={PAGE_MAIN}>
        <EmptyState
          icon={<IconMovie size={32} stroke={1.5} />}
          title={t('content.homeEmpty')}
          hint={t('content.homeEmptyHint')}
        />
      </main>
    );
  }
  return (
    <main className="min-w-0 px-(--gutter-web) pb-20 pt-9">
      {movies[0] ? <Hero movie={movies[0]} /> : null}
      <ContinueRow />
      <HomeSections />
      <ShowRail title={t('nav.series')} shows={shows} />
    </main>
  );
}
