import { formatRuntime, qualityBadge } from '@luma/core';
import { useT } from '@luma/ui';
import { TvDetailScaffold } from '#tv/features/catalog/detail/DetailScaffold';
import { CastRow, EndsAtHint, ListButton, WatchedButton } from '#tv/features/catalog/detail/parts';
import { TvAiSuggestRow } from '#tv/features/catalog/detail/TvAiSuggestRow';
import { useMyList } from '#tv/app/providers/mylist';
import { useWatched } from '#tv/app/providers/watched';
import { useClient, useNav, useParams } from '#tv/app/router';
import { PlayGlyph, TV_PLAY_BTN } from '#tv/shared/TvMedia';
import { useFocusNav } from '#tv/app/useFocusNav';

/** Film detail backdrop, synopsis, metadata, a Lecture button, my-list, an
 * "ends at" hint and the cast. The movie already carries its TMDB metadata from
 * the catalog list, so no extra fetch. */
export function TvMovieDetail() {
  const nav = useNav();
  const { item } = useParams('movie');
  const client = useClient();
  const t = useT();
  const myList = useMyList();
  const watched = useWatched();
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
        <ListButton inList={myList.has(item.id)} onToggle={() => myList.toggle(item.id)} />
        <WatchedButton watched={watched.has(item.id)} onToggle={() => watched.toggle(item.id)} />
      </div>
      <EndsAtHint runtimeMs={item.durationMs} />
      <CastRow cast={item.metadata?.cast} />
      <TvAiSuggestRow id={item.id} />
    </TvDetailScaffold>
  );
}
