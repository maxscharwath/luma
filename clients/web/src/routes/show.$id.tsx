import { useT } from '@luma/ui';
import { createFileRoute, redirect } from '@tanstack/react-router';
import { useMemo } from 'react';
import { TitleDetail } from '#web/features/catalog/title-detail';
import { isAuthed, lumaClient } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { buildTitleView } from '#web/shared/lib/titleView';

export const Route = createFileRoute('/show/$id')({
  loader: async ({ params }) => {
    if (!isAuthed()) throw redirect({ to: '/' });
    const c = lumaClient();
    const [detail, shows] = await Promise.all([c.show(params.id), c.shows()]);
    const show = detail.show;
    const tmdbId = show.metadata?.tmdbId ?? null;
    // The discover overlay (season availability + request state) is fetched only
    // for an enriched show and degrades to null for viewers without
    // `requests.create` (a 403 the server returns before any TMDB call). P2
    // folds this into a single title endpoint.
    const [upNext, discover] = await Promise.all([
      c.upNext(show.id).catch(() => null),
      tmdbId != null ? c.discoverDetail('tv', tmdbId).catch(() => null) : Promise.resolve(null),
    ]);
    const genres = new Set(show.metadata?.genres ?? []);
    const others = shows.filter((s) => s.id !== show.id);
    const related = others.filter((s) => (s.metadata?.genres ?? []).some((g) => genres.has(g)));
    const similarShows = (related.length >= 3 ? related : others).slice(0, 12);
    return { detail, similarShows, upNext, discover };
  },
  component: ShowDetailPage,
});

function ShowDetailPage() {
  const t = useT();
  const { client, user } = useAuth();
  const { detail, similarShows, upNext, discover } = Route.useLoaderData();
  const view = useMemo(
    () =>
      buildTitleView(client, t, user, { source: 'show', detail, similarShows, upNext, discover }),
    [client, t, user, detail, similarShows, upNext, discover],
  );
  return <TitleDetail key={detail.show.id} initial={view} />;
}
