import { type DownloadedSub, isTextSubtitle, type LumaClient, type MediaItem } from '@luma/core';
import { useCallback, useEffect, useMemo, useState } from 'react';

export interface SubView {
  index: number;
  language: string | null;
  url: string | null;
  /** Generated tracks carry a label + id and render an "IA" badge. */
  label?: string;
  subId?: string;
  ai?: boolean;
}

export interface SubtitleSelection {
  /** Renderable (text) subtitle tracks: embedded first, then generated. */
  rendered: SubView[];
  /** Picker options: `null` (off) + each renderable track index. */
  options: (number | null)[];
  /** The active track index, or null when off. */
  active: number | null;
  /** Select a track (or null to turn off). */
  pick: (index: number | null) => void;
  /** Re-fetch the generated-subtitle list (after a generation completes). */
  reload: () => void;
}

/** Resolves an item's renderable subtitle tracks (embedded + on-device generated)
 * and tracks the active selection. The custom <TvSubtitles> layer renders cues
 * itself, so "picking" is just state. */
export function useSubtitleSelection(client: LumaClient, item: MediaItem): SubtitleSelection {
  const [active, setActive] = useState<number | null>(null);
  const [downloaded, setDownloaded] = useState<DownloadedSub[]>([]);
  const [nonce, setNonce] = useState(0);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `nonce` is the reload trigger (bumped by reload()) that forces a re-fetch after a generation completes; it is intentionally a dependency though the body does not read it.
  useEffect(() => {
    let cancelled = false;
    client
      .downloadedSubtitles(item.id)
      .then((d) => !cancelled && setDownloaded(d))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [client, item.id, nonce]);
  const reload = useCallback(() => setNonce((n) => n + 1), []);

  const rendered = useMemo<SubView[]>(() => {
    const embedded: SubView[] = item.subtitles
      .map((s, index) => ({
        index,
        language: s.language,
        url: isTextSubtitle(s.codec) ? client.subtitleUrl(item.id, index) : null,
      }))
      .filter((s) => s.url);
    // Generated tracks get high indices (1000+) so they never collide with embedded.
    const gen: SubView[] = downloaded.map((d, i) => ({
      index: 1000 + i,
      language: d.language,
      url: client.resolveArt(d.url) ?? d.url,
      label: d.label,
      subId: d.id,
      ai: true,
    }));
    return [...embedded, ...gen];
  }, [client, item, downloaded]);

  const options = useMemo<(number | null)[]>(
    () => [null, ...rendered.map((s) => s.index)],
    [rendered],
  );
  const pick = useCallback((index: number | null) => setActive(index), []);

  return { rendered, options, active, pick, reload };
}
