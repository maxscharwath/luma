import { useT } from '@luma/ui';
import { createFileRoute } from '@tanstack/react-router';
import { ContinueRow } from '#web/features/catalog/ContinueRow';
import { Hero, ShowRail } from '#web/features/catalog/cards';
import { HomeSections } from '#web/features/catalog/HomeSections';
import { lumaClient, toMovieView, toShowView } from '#web/shared/lib/api';

export const Route = createFileRoute('/')({
  loader: async () => {
    const c = lumaClient();
    const [movies, shows] = await Promise.all([c.movies(), c.shows()]);
    return {
      movies: movies.map((m) => toMovieView(c, m)),
      shows: shows.map((s) => toShowView(c, s)),
    };
  },
  component: HomePage,
});

function HomePage() {
  const t = useT();
  const { movies, shows } = Route.useLoaderData();
  return (
    <main className="max-w-400 px-(--gutter-web) pb-16 pt-10">
      {movies[0] ? <Hero movie={movies[0]} /> : null}
      <ContinueRow />
      <HomeSections />
      <ShowRail title={t('nav.series')} shows={shows} />
    </main>
  );
}
