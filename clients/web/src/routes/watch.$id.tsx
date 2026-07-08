import { createFileRoute, redirect, useNavigate } from '@tanstack/react-router';
import { Player } from '#web/features/playback/player';
import { isAuthed, lumaClient, toMovieView } from '#web/shared/lib/api';

export const Route = createFileRoute('/watch/$id')({
  loader: async ({ params }) => {
    if (!isAuthed()) throw redirect({ to: '/' });
    const c = lumaClient();
    // The next episode (for the Netflix-style "up next" autoplay) is sequence-based
    // and public, so it loads alongside the item.
    const [item, next] = await Promise.all([c.item(params.id), c.nextEpisode(params.id)]);
    return { item: toMovieView(c, item), next };
  },
  component: WatchPage,
});

function WatchPage() {
  const { item, next } = Route.useLoaderData();
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
