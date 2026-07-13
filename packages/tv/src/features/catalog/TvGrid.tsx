import { posterColors, qualityBadge, qualityBadgeForVideo } from '@luma/core';
import { useT } from '@luma/ui';
import { useMemo } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { type GridCard, TvGrid as PosterGrid } from '#tv/features/catalog/home/TvGrid';
import { useMyList } from '#tv/app/providers/mylist';
import { useWatched } from '#tv/app/providers/watched';
import { useClient, useNav, useParams } from '#tv/app/router';
import { badgeClasses, TvArt } from '#tv/shared/TvMedia';
import { useFocusNav } from '#tv/app/useFocusNav';

/** Full-screen catalogue grid for one section (Films / Séries / Ma liste): a 44%
 * hero over the first title, then an incrementally-rendered 2:3 poster grid.
 * Shares the top nav with Home. */
export function TvGrid() {
  const { kind } = useParams('grid');
  const { movies, shows } = useConnection();
  const client = useClient();
  const t = useT();
  const nav = useNav();
  const myList = useMyList();
  const watched = useWatched();
  const isFilms = kind === 'films';
  const isSeries = kind === 'series';
  useFocusNav({ onBack: nav.back, resetKey: kind });

  const cards = useMemo<GridCard[]>(() => {
    const movieCard = (m: (typeof movies)[number]): GridCard => ({
      id: m.id,
      title: m.title,
      poster: client.posterFor(m),
      colors: posterColors(m.id),
      watched: watched.has(m.id),
      onClick: () => nav.go('movie', { item: m }),
    });
    const showCard = (s: (typeof shows)[number]): GridCard => ({
      id: s.id,
      title: s.title,
      poster: client.showPosterFor(s),
      colors: posterColors(s.id),
      watched: watched.has(s.id),
      progress: s.progress ?? null,
      onClick: () => nav.go('show', { show: s }),
    });
    if (isFilms) return movies.map(movieCard);
    if (isSeries) return shows.map(showCard);
    return [
      ...movies.filter((m) => myList.has(m.id)).map(movieCard),
      ...shows.filter((s) => myList.has(s.id)).map(showCard),
    ];
  }, [isFilms, isSeries, movies, shows, client, nav, myList, watched]);

  let heroMovie: (typeof movies)[number] | undefined;
  if (isFilms) heroMovie = movies[0];
  else if (isSeries) heroMovie = undefined;
  else heroMovie = movies.find((m) => myList.has(m.id));
  let heroShow: (typeof shows)[number] | undefined;
  if (isSeries) heroShow = shows[0];
  else if (heroMovie) heroShow = undefined;
  else heroShow = shows.find((s) => myList.has(s.id));
  const hero = heroMovie ?? heroShow ?? null;
  const heroBackdrop = hero ? (client.backdropFor(hero) ?? client.posterFor(hero)) : null;
  let heroBadge: string | null = null;
  if (heroMovie) heroBadge = qualityBadge(heroMovie);
  else if (heroShow) heroBadge = qualityBadgeForVideo(heroShow.video);
  let label: string;
  if (isFilms) label = t('nav.films');
  else if (isSeries) label = t('nav.series');
  else label = t('nav.myList');
  const empty = kind === 'mylist' && cards.length === 0;

  return (
    <div className="fixed inset-0 flex flex-col overflow-hidden bg-bg">
      <section className="relative flex-[0_0_44%]">
        <TvArt
          src={heroBackdrop}
          colors={hero ? posterColors(hero.id) : ['#1c1c22', '#0a0a0c']}
          position="50% 22%"
        />
        <div className="absolute inset-0 bg-[linear-gradient(90deg,#0A0A0C_6%,transparent_62%),linear-gradient(0deg,#0A0A0C_2%,transparent_52%)]" />
        <TvTopNav active={kind} />
        {hero ? (
          <div className="absolute bottom-7 left-16 max-w-195">
            <div className="mb-3 font-sans text-[13px] font-bold uppercase tracking-[0.22em] text-accent">
              {label} · {cards.length}
            </div>
            <h1 className="m-0 mb-3 font-display text-[clamp(36px,6.2vh,68px)] font-bold leading-[0.98] tracking-[-0.02em]">
              {hero.title}
            </h1>
            <div className="flex flex-wrap items-center gap-2.75 font-sans text-[16px] font-semibold text-muted">
              {hero.metadata?.rating ? (
                <>
                  <span className="font-bold text-accent">{hero.metadata.rating.toFixed(1)}★</span>
                  <span className="text-dim">·</span>
                </>
              ) : null}
              <span>{hero.year ?? ''}</span>
              {heroBadge ? <span className={badgeClasses(heroBadge)}>{heroBadge}</span> : null}
            </div>
          </div>
        ) : (
          <div className="absolute bottom-7 left-16">
            <div className="mb-3 font-sans text-[13px] font-bold uppercase tracking-[0.22em] text-accent">
              {label}
            </div>
          </div>
        )}
      </section>

      {empty ? (
        <div className="flex flex-1 items-center justify-center px-16">
          <p className="max-w-160 text-center font-sans text-[18px] font-medium text-dim">
            {t('content.myListEmpty')}
          </p>
        </div>
      ) : (
        <PosterGrid cards={cards} />
      )}

      <div className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center gap-7.5 bg-[linear-gradient(0deg,rgba(10,10,12,0.85),transparent)] p-4 font-sans text-[13px] font-semibold text-dim">
        <span>{t('content.hintBrowseAll')}</span>
        <span>{t('content.hintRows')}</span>
        <span>
          <b className="font-bold text-accent">{t('content.hintOk')}</b> {t('content.hintOpen')}
        </span>
      </div>
    </div>
  );
}
