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
import { useMemo } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useClient, useNav } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { TvArt } from '#tv/shared/TvMedia';

/** Genre picker: every genre in the library (movies + shows), most common first.
 * Selecting one drills into {@link TvGenreGrid}. Derives the genre list from the
 * already-loaded catalogue no extra request, like {@link TvPerson}. Each card is
 * fronted by the genre's best-rated backdrop, washed in its signature colour. */
export function TvGenres() {
  const { movies, shows } = useConnection();
  const client = useClient();
  const t = useT();
  const nav = useNav();
  useFocusNav({ onBack: nav.back });

  const catalogue = useMemo(() => [...movies, ...shows], [movies, shows]);
  const genres = useMemo(() => collectGenres(catalogue), [catalogue]);
  const showcases = useMemo(() => genreShowcases(catalogue), [catalogue]);

  return (
    <div className="fixed inset-0 flex flex-col overflow-hidden bg-bg animate-[tv-fade-in_0.3s_ease]">
      <header className="px-16 pb-4 pt-28">
        <h1 className="m-0 font-display text-[clamp(34px,5.5vh,60px)] font-bold leading-[0.98] tracking-[-0.02em]">
          {t('nav.genres')}
        </h1>
      </header>

      {genres.length ? (
        <div className="scrollbar-none min-h-0 flex-1 overflow-y-auto px-16 pb-18 pt-2">
          <div className="flex flex-wrap gap-3">
            {genres.map((g) => {
              const pick = showcases.get(g.name);
              return (
                <GenreCard
                  key={g.name}
                  genre={g}
                  count={t('person.titleCount', { count: g.count })}
                  backdrop={pick ? client.backdropFor(pick) : null}
                  onClick={() => nav.go('genre', { name: g.name })}
                />
              );
            })}
          </div>
        </div>
      ) : (
        <div className="flex flex-1 items-center justify-center px-16">
          <p className="max-w-160 text-center font-sans text-[18px] font-medium text-dim">
            {t('genres.empty')}
          </p>
        </div>
      )}

      {/* Persistent nav last in DOM so a genre tile keeps the initial focus. */}
      <TvTopNav active="genres" />
    </div>
  );
}

/** One genre tile: library backdrop (or the genre-colour gradient) under a
 * bottom-heavy wash of the genre's hue. The button's own padding keeps the
 * global amber focus ring clear of the artwork. */
function GenreCard({
  genre,
  count,
  backdrop,
  onClick,
}: Readonly<{ genre: GenreCount; count: string; backdrop: string | null; onClick: () => void }>) {
  return (
    <button
      type="button"
      data-focus=""
      onClick={onClick}
      className="w-85 flex-none cursor-pointer rounded-[20px] border-none bg-transparent p-1.5 text-left outline-none transition-transform focus:scale-[1.04]"
    >
      <div className="relative aspect-video overflow-hidden rounded-[14px] bg-surface-1 shadow-card [contain-intrinsic-size:328px_185px] [content-visibility:auto]">
        <TvArt
          src={sizedImageUrl(backdrop, 328)}
          colors={genreColors(genre.name)}
          position="50% 25%"
        />
        <div className="absolute inset-0" style={{ background: genreTint(genre.name) }} />
        <div className="absolute inset-x-5 bottom-4">
          <div
            className="mb-2 h-1 w-7 rounded-full"
            style={{ background: genreAccent(genre.name) }}
          />
          <div className="font-display text-[23px] font-bold leading-[1.05] text-white">
            {genre.name}
          </div>
          <div className="mt-0.5 font-sans text-[14px] font-semibold text-[rgba(255,255,255,0.72)] tabular-nums">
            {count}
          </div>
        </div>
      </div>
    </button>
  );
}
