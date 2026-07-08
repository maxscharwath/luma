import { useT } from '@luma/ui';
import { createFileRoute } from '@tanstack/react-router';
import { MovieGrid } from '#web/features/catalog/cards';
import { isAuthed, lumaClient, toMovieView } from '#web/shared/lib/api';

export const Route = createFileRoute('/films')({
  loader: async () => {
    if (!isAuthed()) return { movies: [] };
    const c = lumaClient();
    const movies = await c.movies();
    return { movies: movies.map((m) => toMovieView(c, m)) };
  },
  component: FilmsPage,
});

function FilmsPage() {
  const t = useT();
  const { movies } = Route.useLoaderData();
  return (
    <main className="max-w-400 px-(--gutter-web) pb-16 pt-10">
      <h2 className="mb-6 mt-2 font-display text-[28px] font-bold tracking-[-.02em]">
        {t('nav.films')}
      </h2>
      <MovieGrid movies={movies} />
    </main>
  );
}
