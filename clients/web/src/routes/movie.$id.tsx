import { useT } from '@luma/ui';
import { createFileRoute } from '@tanstack/react-router';
import { useMemo } from 'react';
import { TitleDetail } from '#web/features/catalog/titleDetail';
import { lumaClient } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { buildTitleView } from '#web/shared/lib/titleView';

export const Route = createFileRoute('/movie/$id')({
  loader: async ({ params }) => {
    const c = lumaClient();
    // "Titres similaires" prefers content-embedding neighbours, falling back to
    // genre overlap, then any other movie.
    const [item, movies, embed] = await Promise.all([
      c.item(params.id),
      c.movies(),
      c.similar(params.id).catch(() => []),
    ]);
    const genres = new Set(item.metadata?.genres ?? []);
    const others = movies.filter((m) => m.id !== item.id);
    const related = others.filter((m) => (m.metadata?.genres ?? []).some((g) => genres.has(g)));
    let pool = others;
    if (embed.length >= 3) pool = embed;
    else if (related.length >= 3) pool = related;
    return { item, similar: pool.slice(0, 12) };
  },
  component: MovieDetailPage,
});

function MovieDetailPage() {
  const t = useT();
  const { client, user } = useAuth();
  const { item, similar } = Route.useLoaderData();
  const view = useMemo(
    () => buildTitleView(client, t, user, { source: 'movie', item, similar, discover: null }),
    [client, t, user, item, similar],
  );
  return <TitleDetail key={item.id} initial={view} />;
}
