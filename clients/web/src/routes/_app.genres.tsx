import {
  collectGenres,
  type GenreCount,
  genreAccent,
  genreColors,
  genreShowcases,
  genreTint,
  sizedImageUrl,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconCategory } from '@tabler/icons-react';
import { useSuspenseQuery } from '@tanstack/react-query';
import { createFileRoute, Link } from '@tanstack/react-router';
import { useMemo } from 'react';
import { isAuthed } from '#web/shared/lib/api';
import { catalogQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_TITLE, SkeletonRow } from '#web/shared/ui';

export const Route = createFileRoute('/_app/genres')({
  loader: async ({ context: { queryClient } }) => {
    if (!isAuthed()) return;
    await Promise.all([
      queryClient.ensureQueryData(catalogQueries.moviesView()),
      queryClient.ensureQueryData(catalogQueries.showsView()),
    ]);
  },
  pendingComponent: GenresPending,
  component: GenresPage,
});

function GenresPending() {
  const t = useT();
  return (
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{t('nav.genres')}</h1>
      <div className="mt-6">
        <SkeletonRow count={10} />
      </div>
    </main>
  );
}

function GenresPage() {
  const t = useT();
  const { data: movies } = useSuspenseQuery(catalogQueries.moviesView());
  const { data: shows } = useSuspenseQuery(catalogQueries.showsView());

  // Genres are derived from the whole catalogue (movies + shows), already
  // localized server-side, ranked most-common first; each card is fronted by
  // the genre's best-rated backdrop from the library.
  const catalogue = useMemo(() => [...movies, ...shows], [movies, shows]);
  const genres = useMemo(() => collectGenres(catalogue), [catalogue]);
  const showcases = useMemo(() => genreShowcases(catalogue), [catalogue]);

  return (
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{t('nav.genres')}</h1>
      {genres.length === 0 ? (
        <EmptyState icon={<IconCategory size={32} stroke={1.5} />} title={t('genres.empty')} />
      ) : (
        <div className="mt-6 grid grid-cols-2 gap-4 md:grid-cols-3 xl:grid-cols-4">
          {genres.map((g) => (
            <GenreTile
              key={g.name}
              genre={g}
              count={t('person.titleCount', { count: g.count })}
              backdrop={showcases.get(g.name)?.backdrop ?? null}
            />
          ))}
        </div>
      )}
    </main>
  );
}

/** A tappable genre card: library backdrop (or the genre-colour gradient) under
 * a bottom-heavy wash of the genre's signature hue. */
function GenreTile({
  genre,
  count,
  backdrop,
}: Readonly<{ genre: GenreCount; count: string; backdrop: string | null }>) {
  const [c1, c2] = genreColors(genre.name);
  return (
    <Link
      to="/genre/$genre"
      params={{ genre: genre.name }}
      className="group relative block aspect-video overflow-hidden rounded-2xl border border-white/[0.06] no-underline transition-colors hover:border-accent/50 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
      style={{ background: `linear-gradient(150deg, ${c1}, ${c2})` }}
    >
      {backdrop ? (
        <img
          src={sizedImageUrl(backdrop, 420) ?? undefined}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          className="absolute inset-0 h-full w-full object-cover object-[50%_25%] transition-transform duration-500 group-hover:scale-105"
        />
      ) : null}
      <div
        className="pointer-events-none absolute inset-0"
        style={{ background: genreTint(genre.name) }}
      />
      <div className="absolute inset-x-4 bottom-3.5 sm:inset-x-5 sm:bottom-4">
        <div
          className="mb-1.5 h-1 w-6 rounded-full"
          style={{ background: genreAccent(genre.name) }}
        />
        <div className="font-display text-[16px] font-bold leading-tight tracking-[-.01em] text-white sm:text-[19px]">
          {genre.name}
        </div>
        <div className="mt-0.5 text-[12px] font-medium text-white/70 tabular-nums sm:text-[13px]">
          {count}
        </div>
      </div>
    </Link>
  );
}
