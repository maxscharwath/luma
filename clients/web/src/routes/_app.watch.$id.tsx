import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, redirect, useNavigate } from '@tanstack/react-router';
import { Player } from '#web/features/playback/player';
import { isAuthed } from '#web/shared/lib/api';
import { catalogQueries } from '#web/shared/lib/queries';

export const Route = createFileRoute('/_app/watch/$id')({
  loader: async ({ params, context: { queryClient } }) => {
    if (!isAuthed()) throw redirect({ to: '/' });
    // The next episode (for the Netflix-style "up next" autoplay) is sequence-based
    // and public, so it loads alongside the item.
    await queryClient.ensureQueryData(catalogQueries.watch(params.id));
  },
  // Player is fullscreen with its own buffering spinner; a black hold beats a
  // structural skeleton here.
  pendingComponent: () => <div className="fixed inset-0 bg-black" />,
  component: WatchPage,
});

function WatchPage() {
  const { id } = Route.useParams();
  const {
    data: { item, next },
  } = useSuspenseQuery(catalogQueries.watch(id));
  const navigate = useNavigate();
  return (
    <Player
      key={item.id}
      item={item}
      next={next}
      onPlayNext={next ? () => navigate({ to: '/watch/$id', params: { id: next.id } }) : undefined}
      onClose={() => navigate({ to: '/' })}
    />
  );
}
