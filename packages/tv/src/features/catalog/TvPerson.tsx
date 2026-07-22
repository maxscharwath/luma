import type { Metadata } from '@kroma/core';
import { creditsPerson, personInvolvement, posterColors, roleLabels } from '@kroma/core';
import { useT } from '@kroma/ui';
import { Avatar, Box, radius, Txt, useFocusNav } from '@kroma/ui/kit';
import { useMemo } from 'react';
import { useConnection } from '#tv/app/providers/connection';
import { useClient, useNav, useParams } from '#tv/app/router';
import { TvTopNav } from '#tv/features/catalog/home/TopNav';
import { type GridCard, TvGrid as PosterGrid } from '#tv/features/catalog/home/TvGrid';

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
    <Box fill bg="bg" overflow="hidden">
      {/* Header sits below the persistent nav bar (its top padding clears it);
          Back is the remote key, so no separate hint. */}
      <Box row align="center" gap={24} px={64} pt={112} pb={24}>
        <Avatar name={name} src={photo} size={96} radius={radius.pill} />
        <Box style={{ minWidth: 0 }} gap={8}>
          {roles.length ? (
            <Txt style={SECTION} color="accent">
              {roles.join(' · ')}
            </Txt>
          ) : null}
          <Txt variant="hero" style={TITLE}>
            {name}
          </Txt>
          <Txt style={{ fontSize: 16, fontWeight: '600' }} color="textMuted">
            {t('person.titleCount', { count: cards.length })}
          </Txt>
        </Box>
      </Box>

      {cards.length ? (
        <PosterGrid cards={cards} />
      ) : (
        <Box flex center px={64}>
          <Txt style={EMPTY} color="textDim">
            {t('person.empty')}
          </Txt>
        </Box>
      )}

      {/* Persistent nav last in the tree so a poster keeps the initial focus. */}
      <TvTopNav />
    </Box>
  );
}

const SECTION = {
  fontSize: 13,
  fontWeight: '700' as const,
  letterSpacing: 2.86,
  textTransform: 'uppercase' as const,
};
// clamp(34px, 5.5vh, 60px) resolves to 59px on the fixed 1080-tall stage.
const TITLE = { fontSize: 59, lineHeight: 58, fontWeight: '700' as const, letterSpacing: -1.18 };
const EMPTY = {
  fontSize: 18,
  fontWeight: '500' as const,
  textAlign: 'center' as const,
  maxWidth: 640,
};
