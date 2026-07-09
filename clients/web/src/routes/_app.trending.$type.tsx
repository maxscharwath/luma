import { createFileRoute, redirect } from '@tanstack/react-router';
import { TrendingPage } from '#web/features/requests/trending-page';

export const Route = createFileRoute('/_app/trending/$type')({
  beforeLoad: ({ params }) => {
    if (params.type !== 'movie' && params.type !== 'tv') {
      throw redirect({ to: '/search' });
    }
  },
  component: TrendingRoute,
});

function TrendingRoute() {
  const { type } = Route.useParams();
  // Remount (resetting pagination to page 1) when switching movie <-> tv.
  return <TrendingPage key={type} type={type as 'movie' | 'tv'} />;
}
