import { useT } from '@luma/ui';
import { createFileRoute } from '@tanstack/react-router';
import { ContinueRow } from '#web/components/ContinueRow';
import { Hero, MovieRail, ShowRail } from '#web/components/cards';
import { lumaClient, toMovieView, toShowView } from '#web/lib/api';

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
  const hdr = movies.filter((m) => m.video?.hdr);
  return (
    <main className="max-w-400 px-(--gutter-web) pb-16 pt-10">
      {movies[0] ? <Hero movie={movies[0]} /> : null}
      <ContinueRow />
      {hdr.length >= 3 ? <MovieRail title={t('content.hdr4k')} movies={hdr} /> : null}
      <MovieRail title={t('nav.films')} movies={movies} />
      <ShowRail title={t('nav.series')} shows={shows} />
    </main>
  );
}
