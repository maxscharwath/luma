import { useT } from '@kroma/ui';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, redirect } from '@tanstack/react-router';
import { TitleDetail } from '#web/features/catalog/title-detail';
import { isAuthed } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { discoverQueries } from '#web/shared/lib/queries';
import { buildTitleView } from '#web/shared/lib/titleView';
import { DetailSkeleton } from '#web/shared/ui';

export const Route = createFileRoute('/_app/discover/$type/$tmdbId')({
  loader: async ({ params, context: { queryClient } }) => {
    if (!isAuthed()) throw redirect({ to: '/' });
    if (params.type !== 'tv' && params.type !== 'movie') throw redirect({ to: '/search' });
    const kind = params.type as 'movie' | 'tv';
    const detail = await queryClient.ensureQueryData(
      discoverQueries.detail(kind, Number(params.tmdbId)),
    );
    // Owned (fully OR partially) → the canonical local fiche, which now overlays
    // the season gaps itself. Only not-owned titles render on the discover route.
    if (detail.localId) {
      throw redirect({
        to: detail.kind === 'show' ? '/show/$id' : '/movie/$id',
        params: { id: detail.localId },
      });
    }
  },
  pendingComponent: DetailSkeleton,
  component: DiscoverRoute,
});

function DiscoverRoute() {
  const t = useT();
  const { client, user } = useAuth();
  const { type, tmdbId } = Route.useParams();
  const kind = type as 'movie' | 'tv';
  const { data: detail } = useSuspenseQuery(discoverQueries.detail(kind, Number(tmdbId)));
  const view = buildTitleView(client, t, user, { source: 'discover', detail });
  return <TitleDetail key={detail.tmdbId} initial={view} />;
}
