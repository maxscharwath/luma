import { useT } from '@luma/ui';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, redirect } from '@tanstack/react-router';
import { TitleDetail } from '#web/features/catalog/title-detail';
import { isAuthed } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { catalogQueries } from '#web/shared/lib/queries';
import { buildTitleView } from '#web/shared/lib/titleView';
import { DetailSkeleton } from '#web/shared/ui';

export const Route = createFileRoute('/_app/show/$id')({
  loader: async ({ params, context: { queryClient } }) => {
    if (!isAuthed()) throw redirect({ to: '/' });
    await queryClient.ensureQueryData(catalogQueries.showBundle(params.id));
  },
  pendingComponent: DetailSkeleton,
  component: ShowDetailPage,
});

function ShowDetailPage() {
  const t = useT();
  const { client, user } = useAuth();
  const { id } = Route.useParams();
  const {
    data: { detail, similarShows, upNext, discover },
  } = useSuspenseQuery(catalogQueries.showBundle(id));
  const view = buildTitleView(client, t, user, {
    source: 'show',
    detail,
    similarShows,
    upNext,
    discover,
  });
  return <TitleDetail key={detail.show.id} initial={view} />;
}
