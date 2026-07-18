import { formatRuntime, type MediaItem, metaLine } from '@kroma/core';
import type { UpNextData, UpNextItem } from '@kroma/ui';
import { useEffect, useMemo, useState } from 'react';
import { kromaClient } from '#web/shared/lib/api';

/** Map a media item to an up-next card (16:9 backdrop, runtime, context line). */
function toCard(item: MediaItem): UpNextItem {
  const c = kromaClient();
  const isEp = item.season != null && item.episode != null;
  return {
    id: item.id,
    title: isEp ? (item.episodeTitle ?? item.title) : item.title,
    subtitle: isEp ? `S${item.season} E${item.episode}` : metaLine(item),
    posterUrl: c.backdropFor(item) ?? c.posterFor(item),
    durationLabel: formatRuntime(item.durationMs),
    categoryLabel: item.metadata?.genres?.[0],
  };
}

/** Stable empty default so the memo below doesn't recompute for a movie. */
const NO_EPISODES: MediaItem[] = [];

/**
 * "À suivre" data (§10) for the web player: the upcoming episodes plus
 * content-similar recommendations, mapped to the shared up-next card shape.
 */
export function useWebUpNext(item: MediaItem, following: MediaItem[] = NO_EPISODES): UpNextData {
  const [similar, setSimilar] = useState<MediaItem[]>([]);
  // Recommend against the SHOW when watching an episode (episodes carry no
  // embedding of their own, so similar(episodeId) would be empty); a movie
  // recommends against itself.
  const recoId = item.showId ?? item.id;
  useEffect(() => {
    let cancelled = false;
    kromaClient()
      .similar(recoId)
      .then((list) => !cancelled && setSimilar(list))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [recoId]);

  return useMemo(
    () => ({
      nextEpisodes: following.map(toCard),
      recommendations: similar.slice(0, 18).map(toCard),
    }),
    [following, similar],
  );
}
