import { isTextSubtitle, type LumaClient, type MediaItem } from '@luma/core';
import { useCallback, useMemo, useState } from 'react';

export interface SubView {
  index: number;
  language: string | null;
  url: string | null;
}

export interface SubtitleSelection {
  /** Renderable (text) subtitle tracks. */
  rendered: SubView[];
  /** Picker options: `null` (off) + each renderable track index. */
  options: (number | null)[];
  /** The active track index, or null when off. */
  active: number | null;
  /** Select a track (or null to turn off). */
  pick: (index: number | null) => void;
}

/** Resolves an item's renderable subtitle tracks and tracks the active selection.
 * The custom <TvSubtitles> layer renders cues itself, so "picking" is just state. */
export function useSubtitleSelection(client: LumaClient, item: MediaItem): SubtitleSelection {
  const [active, setActive] = useState<number | null>(null);

  const rendered = useMemo<SubView[]>(
    () =>
      item.subtitles
        .map((s, index) => ({
          index,
          language: s.language,
          url: isTextSubtitle(s.codec) ? client.subtitleUrl(item.id, index) : null,
        }))
        .filter((s) => s.url),
    [client, item],
  );

  const options = useMemo<(number | null)[]>(
    () => [null, ...rendered.map((s) => s.index)],
    [rendered],
  );
  const pick = useCallback((index: number | null) => setActive(index), []);

  return { rendered, options, active, pick };
}
