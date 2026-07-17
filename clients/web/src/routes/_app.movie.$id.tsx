import { useT } from '@kroma/ui';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, redirect } from '@tanstack/react-router';
import { TitleDetail } from '#web/features/catalog/title-detail';
import { isAuthed } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { catalogQueries } from '#web/shared/lib/queries';
import { buildTitleView } from '#web/shared/lib/titleView';
import { DetailSkeleton } from '#web/shared/ui';

export const Route = createFileRoute('/_app/movie/$id')({
  loader: async ({ params, context: { queryClient } }) => {
    // Catalogue is auth-gated: a signed-out deep-link goes to the gate (the
    // AuthGate overlay covers /, and its loader is guarded too).
    if (!isAuthed()) throw redirect({ to: '/' });
    // Warm the cache the component reads from (item + full list for the
    // similar-items fallback + embedding neighbours).
    await Promise.all([
      queryClient.ensureQueryData(catalogQueries.item(params.id)),
      queryClient.ensureQueryData(catalogQueries.movies()),
      queryClient.ensureQueryData(catalogQueries.similar(params.id)),
    ]);
  },
  pendingComponent: DetailSkeleton,
  component: MovieDetailPage,
});

function MovieDetailPage() {
  const t = useT();
  const { client, user } = useAuth();
  const { id } = Route.useParams();
  const { data: item } = useSuspenseQuery(catalogQueries.item(id));
  const { data: movies } = useSuspenseQuery(catalogQueries.movies());
  const { data: embed } = useSuspenseQuery(catalogQueries.similar(id));

  // "Titres similaires" prefers content-embedding neighbours, falling back to
  // genre overlap, then any other movie.
  const genres = new Set(item.metadata?.genres ?? []);
  const others = movies.filter((m) => m.id !== item.id);
  const related = others.filter((m) => (m.metadata?.genres ?? []).some((g) => genres.has(g)));
  let pool = others;
  if (embed.length >= 3) pool = embed;
  else if (related.length >= 3) pool = related;
  const similar = pool.slice(0, 12);

  const view = buildTitleView(client, t, user, { source: 'movie', item, similar, discover: null });
  return <TitleDetail key={item.id} initial={view} />;
}
