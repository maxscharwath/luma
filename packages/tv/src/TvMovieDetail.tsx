import { formatRuntime, qualityBadge } from '@luma/core';
import { useT } from '@luma/ui';
import { TvDetailScaffold } from '#tv/detail/DetailScaffold';
import { useClient, useNav, useParams } from '#tv/router';
import { PlayGlyph, TV_PLAY_BTN } from '#tv/TvMedia';
import { useFocusNav } from '#tv/useFocusNav';

/** Film detail — backdrop, synopsis, metadata and a Lecture button. The movie
 * already carries its TMDB metadata from the catalog list, so no extra fetch.
 * Props-free: reads its target from the route, navigates via the router. */
export function TvMovieDetail() {
  const nav = useNav();
  const { item } = useParams('movie');
  const client = useClient();
  const t = useT();
  useFocusNav({ onBack: nav.back });

  const meta = item.metadata;
  const metaLong = [
    item.year ? String(item.year) : null,
    formatRuntime(item.durationMs),
    meta?.genres?.[0],
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <TvDetailScaffold
      id={item.id}
      kind={t('content.film')}
      title={item.title}
      backdrop={client.backdropFor(item) ?? client.posterFor(item)}
      rating={meta?.rating}
      meta={metaLong}
      badge={qualityBadge(item)}
      overview={meta?.overview}
    >
      <div className="flex items-center gap-4">
        <button className={TV_PLAY_BTN} data-focus="" onClick={() => nav.go('player', { item })}>
          <PlayGlyph />
          {t('player.play')}
        </button>
      </div>
    </TvDetailScaffold>
  );
}
