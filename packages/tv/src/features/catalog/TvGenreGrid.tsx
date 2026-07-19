import {
  compareTitles,
  hasGenre,
  type MediaItem,
  posterColors,
  type Show,
  type SortMode,
} from '@kroma/core';
import { useT } from '@kroma/ui';
import { useMemo } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useClient, useNav, useParams } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { type GridCard, TvGrid as PosterGrid } from '#tv/features/catalog/home/TvGrid';

// Best-known titles first (rating, then year) the same ranking as the person grid.
const SORT: SortMode = 'rating';

/** Every movie + show in one genre (reached from {@link TvGenres}). Filters the
 * already-loaded catalogue locally, ranked best-rated first. */
export function TvGenreGrid() {
  const { name } = useParams('genre');
  const { movies, shows } = useConnection();
  const client = useClient();
  const t = useT();
  const nav = useNav();
  useFocusNav({ onBack: nav.back, resetKey: name });

  const cards = useMemo<GridCard[]>(() => {
    const movieCard = (m: MediaItem): GridCard => ({
      id: m.id,
      title: m.title,
      poster: client.posterFor(m),
      colors: posterColors(m.id),
      onClick: () => nav.go('movie', { item: m }),
    });
    const showCard = (s: Show): GridCard => ({
      id: s.id,
      title: s.title,
      poster: client.showPosterFor(s),
      colors: posterColors(s.id),
      progress: s.progress ?? null,
      onClick: () => nav.go('show', { show: s }),
    });
    const tagged: { item: MediaItem | Show; card: GridCard }[] = [
      ...movies.filter((m) => hasGenre(m, name)).map((m) => ({ item: m, card: movieCard(m) })),
      ...shows.filter((s) => hasGenre(s, name)).map((s) => ({ item: s, card: showCard(s) })),
    ];
    const cmp = compareTitles(SORT);
    return [...tagged].sort((a, b) => cmp(a.item, b.item)).map((x) => x.card);
  }, [movies, shows, name, client, nav]);

  return (
    <div className="fixed inset-0 flex flex-col overflow-hidden bg-bg animate-[tv-fade-in_0.3s_ease]">
      <header className="px-16 pb-6 pt-28">
        <div className="mb-2 font-sans text-[13px] font-bold uppercase tracking-[0.22em] text-accent">
          {t('nav.genres')}
        </div>
        <h1 className="m-0 font-display text-[clamp(34px,5.5vh,60px)] font-bold leading-[0.98] tracking-[-0.02em]">
          {name}
        </h1>
        <div className="mt-2 font-sans text-[16px] font-semibold text-muted">
          {t('person.titleCount', { count: cards.length })}
        </div>
      </header>

      {cards.length ? (
        <PosterGrid cards={cards} />
      ) : (
        <div className="flex flex-1 items-center justify-center px-16">
          <p className="max-w-160 text-center font-sans text-[18px] font-medium text-dim">
            {t('genres.empty')}
          </p>
        </div>
      )}

      {/* Persistent nav last in DOM so a poster keeps the initial focus. */}
      <TvTopNav />
    </div>
  );
}
