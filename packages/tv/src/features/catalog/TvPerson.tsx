import type { Metadata } from '@kroma/core';
import { creditsPerson, personInvolvement, posterColors, roleLabels } from '@kroma/core';
import { useT } from '@kroma/ui';
import { useMemo, useState } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useClient, useNav, useParams } from '#tv/app/router';
import { useFocusNav } from '#tv/app/useFocusNav';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { type GridCard, TvGrid as PosterGrid } from '#tv/features/catalog/home/TvGrid';
import { gradFor, initials } from '#tv/shared/ui';

/** Everything one cast/crew person is credited in reached by selecting a face
 * in a detail page's "Distribution" rail. Filters the already-loaded catalogue
 * locally (no extra request), ranked best-known work first. */
export function TvPerson() {
  const { name } = useParams('person');
  const { movies, shows } = useConnection();
  const client = useClient();
  const t = useT();
  const nav = useNav();
  useFocusNav({ onBack: nav.back, resetKey: name });

  const { cards, involvement } = useMemo(() => {
    const mine = (meta?: Metadata | null) => creditsPerson(meta, name);
    const rank = (a: { rating: number; year: number }, b: { rating: number; year: number }) =>
      b.rating - a.rating || b.year - a.year;

    const matchedMovies = movies.filter((m) => mine(m.metadata));
    const matchedShows = shows.filter((s) => mine(s.metadata));

    const movieCards = matchedMovies.map((m) => ({
      sort: { rating: m.metadata?.rating ?? 0, year: m.year ?? 0 },
      card: {
        id: m.id,
        title: m.title,
        poster: client.posterFor(m),
        colors: posterColors(m.id),
        onClick: () => nav.go('movie', { item: m }),
      } satisfies GridCard,
    }));
    const showCards = matchedShows.map((s) => ({
      sort: { rating: s.metadata?.rating ?? 0, year: s.year ?? 0 },
      card: {
        id: s.id,
        title: s.title,
        poster: client.showPosterFor(s),
        colors: posterColors(s.id),
        onClick: () => nav.go('show', { show: s }),
      } satisfies GridCard,
    }));

    const cards = [...movieCards, ...showCards]
      .sort((a, b) => rank(a.sort, b.sort))
      .map((c) => c.card);
    const metas = [...matchedMovies, ...matchedShows].map((it) => it.metadata);
    return { cards, involvement: personInvolvement(metas, name) };
  }, [movies, shows, name, client, nav]);

  const photo = client.resolveArt(involvement.profileUrl);
  const roles = roleLabels(t, involvement);

  return (
    <div className="fixed inset-0 flex flex-col overflow-hidden bg-bg animate-[tv-fade-in_0.3s_ease]">
      {/* Header sits below the persistent nav bar (pt clears it); Back is the
          remote key, so no separate hint. */}
      <header className="flex items-center gap-6 px-16 pb-6 pt-28">
        <PersonAvatar photo={photo} name={name} />
        <div className="min-w-0">
          {roles.length ? (
            <div className="mb-2 font-sans text-[13px] font-bold uppercase tracking-[0.22em] text-accent">
              {roles.join(' · ')}
            </div>
          ) : null}
          <h1 className="m-0 font-display text-[clamp(34px,5.5vh,60px)] font-bold leading-[0.98] tracking-[-0.02em]">
            {name}
          </h1>
          <div className="mt-2 font-sans text-[16px] font-semibold text-muted">
            {t('person.titleCount', { count: cards.length })}
          </div>
        </div>
      </header>

      {cards.length ? (
        <PosterGrid cards={cards} />
      ) : (
        <div className="flex flex-1 items-center justify-center px-16">
          <p className="max-w-160 text-center font-sans text-[18px] font-medium text-dim">
            {t('person.empty')}
          </p>
        </div>
      )}

      {/* Persistent nav last in DOM so a poster keeps the initial focus. */}
      <TvTopNav />
    </div>
  );
}

/** Round headshot: the photo (over its gradient placeholder) or initials. */
function PersonAvatar({ photo, name }: Readonly<{ photo: string | null; name: string }>) {
  const [failed, setFailed] = useState(false);
  const showImg = Boolean(photo) && !failed;
  return (
    <div
      className="relative flex h-24 w-24 flex-none items-center justify-center overflow-hidden rounded-full font-display text-[32px] font-bold text-[rgba(255,255,255,0.9)] shadow-card"
      style={{ background: gradFor(name) }}
    >
      <div className="absolute inset-0 bg-[radial-gradient(70%_60%_at_50%_22%,rgba(255,255,255,0.2),transparent_60%)]" />
      {showImg ? (
        <img
          src={photo ?? undefined}
          alt=""
          onError={() => setFailed(true)}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : (
        initials(name)
      )}
    </div>
  );
}
