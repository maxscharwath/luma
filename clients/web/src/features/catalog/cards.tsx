import { metaLine, posterColors, type Section } from '@luma/core';
import { useT } from '@luma/ui';
import { useNavigate } from '@tanstack/react-router';
import { memo } from 'react';
import type { MovieView, ShowView } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { useWatched } from '#web/shared/lib/watched';
import { Badge, Button, Poster, Rail } from '#web/shared/ui';

type HeroBadge = '4K' | 'HDR' | 'H.265';

function heroBadges(movie: MovieView): HeroBadge[] {
  const v = movie.video;
  if (!v) return [];
  const out: HeroBadge[] = [];
  if ((v.width ?? 0) >= 3840) out.push('4K');
  if (v.hdr) out.push('HDR');
  if (v.codec === 'hevc') out.push('H.265');
  return out;
}

const SECTION_TITLE = 'mb-5 mt-10 font-display text-[22px] font-bold tracking-[-.02em] text-text';

/** Full-bleed featured banner TMDB backdrop as cinematic art, bled to the
 * content edges (cancels the page gutter) and faded into the rails below. */
export function Hero({ movie }: Readonly<{ movie: MovieView }>) {
  const t = useT();
  const navigate = useNavigate();
  const colors = posterColors(movie.id);
  const bg = movie.backdrop
    ? `url("${movie.backdrop}")`
    : `linear-gradient(158deg, ${colors[0]}, ${colors[1]})`;
  const meta = movie.metadata;
  const badges = heroBadges(movie);

  return (
    <div
      className="relative -mx-(--gutter-web) -mt-9 mb-8 flex min-h-[52vw] flex-col justify-end overflow-hidden px-(--gutter-web) pb-10 pt-10 sm:min-h-115 sm:pt-16"
      style={{ backgroundImage: bg, backgroundSize: 'cover', backgroundPosition: 'center 18%' }}
    >
      <div className="pointer-events-none absolute inset-0 animate-[luma-breathe_7s_var(--ease-out)_infinite] bg-[radial-gradient(58%_68%_at_72%_32%,rgba(242,180,66,.16),transparent_62%)]" />
      <div className="pointer-events-none absolute inset-0 bg-[linear-gradient(90deg,var(--luma-bg)_6%,rgba(10,10,12,.35)_42%,transparent_64%),linear-gradient(0deg,var(--luma-bg)_2%,transparent_46%)]" />

      <div className="relative max-w-170">
        <div className="mb-3.5 inline-flex items-center gap-1.75 text-[12px] font-bold uppercase tracking-[.22em] text-accent">
          {t('content.featured')}
        </div>
        <h1 className="mb-3.5 font-display text-[clamp(34px,6vw,66px)] font-bold leading-[.98] tracking-[-.02em]">
          {movie.title}
        </h1>
        <div className="mb-4 flex flex-wrap items-center gap-3 text-[13px] font-medium text-muted max-sm:text-[15px]">
          {meta?.rating ? (
            <span className="font-semibold text-accent">{meta.rating.toFixed(1)}★</span>
          ) : null}
          <span>{metaLine(movie)}</span>
          {badges.map((b) => (
            <Badge key={b} tone={b}>
              {b}
            </Badge>
          ))}
        </div>
        {meta?.overview ? (
          <p className="mb-5 line-clamp-3 max-w-135 text-[16px] leading-[1.55] text-text max-sm:text-[17px]">
            {meta.overview}
          </p>
        ) : null}
        <div className="flex flex-wrap gap-3.5">
          <Button onClick={() => navigate({ to: '/watch/$id', params: { id: movie.id } })}>
            {t('content.play')}
          </Button>
          <Button
            variant="glass"
            onClick={() => navigate({ to: '/movie/$id', params: { id: movie.id } })}
          >
            {t('content.moreInfo')}
          </Button>
        </div>
      </div>
    </div>
  );
}

// Cards are memo()d: a home page renders hundreds of them, and without memo any
// parent state change (hover, a poll refetch, router transitions) re-renders
// every card. A watched toggle still re-renders them (context), by design.
const MoviePoster = memo(function MoviePoster({ item }: Readonly<{ item: MovieView }>) {
  const t = useT();
  const navigate = useNavigate();
  const { isWatched, toggleWatched } = useWatched();
  return (
    <Poster
      title={item.title}
      genre={t('content.film')}
      colors={posterColors(item.id)}
      poster={item.poster}
      watched={isWatched(item.id)}
      onToggleWatched={() => toggleWatched(item.id)}
      onClick={() => navigate({ to: '/movie/$id', params: { id: item.id } })}
    />
  );
});

const ShowPoster = memo(function ShowPoster({ show }: Readonly<{ show: ShowView }>) {
  const t = useT();
  const navigate = useNavigate();
  const { isWatched, toggleWatched } = useWatched();
  return (
    <Poster
      title={show.title}
      genre={t('content.seasonCount', { count: show.seasonCount })}
      colors={posterColors(show.id)}
      poster={show.poster}
      progress={show.progress ?? null}
      watched={isWatched(show.id)}
      onToggleWatched={() => toggleWatched(show.id)}
      onClick={() => navigate({ to: '/show/$id', params: { id: show.id } })}
    />
  );
});

/** One item of a server-built [`Section`] (movie or show). */
type SectionEntry = Section['items'][number];

/** Render a server section entry (movie or show) with the same watched badge +
 * show-progress affordances as the catalogue grids. Used by the home rows and the
 * per-title "Suggestions IA" rail so those rails stay consistent with the grids
 * (the poster URL is resolved through the client, matching those rails). */
export const SectionPoster = memo(function SectionPoster({
  entry,
  width,
}: Readonly<{ entry: SectionEntry; width?: number }>) {
  const t = useT();
  const navigate = useNavigate();
  const { client } = useAuth();
  const { isWatched, toggleWatched } = useWatched();
  if (entry.type === 'show') {
    const { show } = entry;
    return (
      <Poster
        title={show.title}
        genre={show.metadata?.genres?.[0] ?? t('content.series')}
        colors={posterColors(show.id)}
        poster={client.showPosterFor(show)}
        width={width}
        progress={show.progress ?? null}
        watched={isWatched(show.id)}
        onToggleWatched={() => toggleWatched(show.id)}
        onClick={() => navigate({ to: '/show/$id', params: { id: show.id } })}
      />
    );
  }
  const { item } = entry;
  return (
    <Poster
      title={item.title}
      genre={item.metadata?.genres?.[0] ?? t('content.film')}
      colors={posterColors(item.id)}
      poster={client.posterFor(item)}
      width={width}
      watched={isWatched(item.id)}
      onToggleWatched={() => toggleWatched(item.id)}
      onClick={() => navigate({ to: '/movie/$id', params: { id: item.id } })}
    />
  );
});

export function MovieRail({ title, movies }: Readonly<{ title: string; movies: MovieView[] }>) {
  if (movies.length === 0) return null;
  return (
    <section>
      <h2 className={SECTION_TITLE}>{title}</h2>
      <Rail label={title}>
        {movies.map((item) => (
          <MoviePoster key={item.id} item={item} />
        ))}
      </Rail>
    </section>
  );
}

export function ShowRail({ title, shows }: Readonly<{ title: string; shows: ShowView[] }>) {
  if (shows.length === 0) return null;
  return (
    <section>
      <h2 className={SECTION_TITLE}>{title}</h2>
      <Rail label={title}>
        {shows.map((show) => (
          <ShowPoster key={show.id} show={show} />
        ))}
      </Rail>
    </section>
  );
}

// Poster grid: auto-fill columns at least one card wide, stretched to fill the
// row (no dead right edge on phones). `*:w-full!` overrides the tiles' inline
// default width so the grid tracks size them.
const GRID =
  'grid grid-cols-[repeat(auto-fill,minmax(min(var(--card-w),100%),1fr))] gap-x-4.5 gap-y-6 *:w-full!';

export function MovieGrid({ movies }: Readonly<{ movies: MovieView[] }>) {
  return (
    <div className={GRID}>
      {movies.map((item) => (
        <MoviePoster key={item.id} item={item} />
      ))}
    </div>
  );
}

export function ShowGrid({ shows }: Readonly<{ shows: ShowView[] }>) {
  return (
    <div className={GRID}>
      {shows.map((show) => (
        <ShowPoster key={show.id} show={show} />
      ))}
    </div>
  );
}

/** A movie or a show, tagged so a mixed list (e.g. one person's filmography)
 * renders each tile with the right poster + navigation. */
export type CatalogEntry = { kind: 'movie'; movie: MovieView } | { kind: 'show'; show: ShowView };

/** A grid mixing movies and shows in the given order (server-ranked). */
export function CatalogGrid({ entries }: Readonly<{ entries: CatalogEntry[] }>) {
  return (
    <div className={GRID}>
      {entries.map((e) =>
        e.kind === 'movie' ? (
          <MoviePoster key={e.movie.id} item={e.movie} />
        ) : (
          <ShowPoster key={e.show.id} show={e.show} />
        ),
      )}
    </div>
  );
}
