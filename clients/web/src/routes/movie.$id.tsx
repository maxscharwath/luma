import { formatRuntime, qualityBadge, type Translate } from '@luma/core';
import { useT } from '@luma/ui';
import { createFileRoute, useNavigate } from '@tanstack/react-router';
import { AiSuggestRail } from '#web/features/catalog/AiSuggestRail';
import {
  audioString,
  CastRail,
  DetailHero,
  directorsOf,
  langName,
  qualityBadges,
  type SimilarItem,
  SimilarRail,
  subString,
} from '#web/features/catalog/detail';
import { lumaClient, type MovieView, toMovieView } from '#web/shared/lib/api';
import { useMyList } from '#web/shared/lib/mylist';
import { useWatched } from '#web/shared/lib/watched';

export const Route = createFileRoute('/movie/$id')({
  loader: async ({ params }) => {
    const c = lumaClient();
    // "Titres similaires" prefers the content-embedding neighbours (ranked by the
    // embedder semantic once MiniLM is enabled, genre-guarded server-side). Fall
    // back to a plain genre-overlap when the title isn't embedded / too few match.
    const [item, movies, embed] = await Promise.all([
      c.item(params.id),
      c.movies(),
      c.similar(params.id).catch(() => []),
    ]);
    const movie = toMovieView(c, item);
    const genres = new Set(movie.metadata?.genres ?? []);
    const others = movies.filter((m) => m.id !== movie.id);
    const related = others.filter((m) => (m.metadata?.genres ?? []).some((g) => genres.has(g)));
    const base =
      embed.length >= 3 ? embed : (related.length >= 3 ? related : others).slice(0, 12);
    const similar: SimilarItem[] = base.slice(0, 12).map((m) => ({
      id: m.id,
      title: m.title,
      // Empty string → the component fills the localized "Movie" fallback.
      genre: m.metadata?.genres?.[0] ?? '',
      badge: qualityBadge(m),
      poster: c.posterFor(m),
    }));
    return { movie, similar };
  },
  component: MovieDetailPage,
});

/** "2024 · 2h08 · Français" year, runtime, primary audio language. */
function metaLong(t: Translate, movie: MovieView): string {
  const parts: string[] = [];
  if (movie.year) parts.push(String(movie.year));
  const rt = formatRuntime(movie.durationMs);
  if (rt) parts.push(rt);
  const lang = langName(t, movie.audio?.language);
  if (lang) parts.push(lang);
  return parts.join(' · ');
}

function MovieDetailPage() {
  const t = useT();
  const { movie, similar } = Route.useLoaderData();
  const navigate = useNavigate();
  const { isWatched, toggleWatched } = useWatched();
  const { inList, toggle: toggleList } = useMyList();
  const meta = movie.metadata;
  const genres = meta?.genres ?? [];

  return (
    <main className="animate-[fade-in_.4s_ease] pb-16">
      <DetailHero
        art={{ id: movie.id, backdrop: movie.backdrop, poster: movie.poster }}
        overline={genres.length ? genres.slice(0, 3).join(' · ') : t('content.film')}
        title={movie.title}
        rating={meta?.rating}
        meta={metaLong(t, movie)}
        badges={qualityBadges(movie.video)}
        directors={directorsOf(meta)}
        tagline={meta?.tagline}
        overview={meta?.overview}
        audio={audioString(t, movie)}
        subtitles={subString(t, movie)}
        playable={movie}
        watched={isWatched(movie.id)}
        onToggleWatched={() => toggleWatched(movie.id)}
        inList={inList(movie.id)}
        onToggleList={() => toggleList(movie.id)}
        onBack={() => navigate({ to: '/' })}
        onPlay={() => navigate({ to: '/watch/$id', params: { id: movie.id } })}
      />
      <CastRail cast={meta?.cast ?? []} />
      <SimilarRail
        title={t('content.similarTitles')}
        items={similar.map((s) => ({ ...s, genre: s.genre || t('content.film') }))}
        onOpen={(id) => navigate({ to: '/movie/$id', params: { id } })}
      />
      <AiSuggestRail id={movie.id} />
    </main>
  );
}
