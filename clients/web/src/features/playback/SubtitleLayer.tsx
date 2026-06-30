import { activeCueText, type Cue, parseVtt } from '@luma/core';
import { memo, type RefObject, useEffect, useRef, useState } from 'react';
import { type SubtitleStyle, subtitleCss } from '#web/features/playback/subtitleStyle';
import type { SubtitleView } from '#web/shared/lib/api';

/**
 * Custom subtitle renderer. Fetches the track's WebVTT itself (CORS-friendly,
 * unlike cross-origin `<track>` elements), parses it once, and renders the
 * active cue synced to playback fully styleable, no native-track quirks.
 *
 * Performance: cue lookup is O(1) amortised via a moving pointer (subtitles are
 * monotonic in time); a binary search re-syncs after a seek. We only re-render
 * when the visible line actually changes.
 */
function SubtitleLayerImpl({
  videoRef,
  rendered,
  activeIndex,
  style,
  raised,
  baseSec,
}: Readonly<{
  videoRef: RefObject<HTMLVideoElement>;
  rendered: SubtitleView[];
  activeIndex: number | null;
  style: SubtitleStyle;
  raised: boolean;
  /** Absolute-position offset: cues are at absolute times but the HLS element
   * clock is relative to the remux anchor, so look up at `baseSec + currentTime`. */
  baseSec: number;
}>) {
  const [cues, setCues] = useState<Cue[]>([]);
  const [text, setText] = useState('');
  const pointer = useRef(0);

  // The active track's WebVTT URL a primitive, used as the effect dependency
  // so a fresh `rendered` array reference on every parent render (Player
  // re-renders ~4×/s from `timeupdate`) does NOT re-trigger a fetch. Depending on
  // the array identity blanked + reloaded the line each tick → flicker.
  const activeUrl =
    activeIndex == null ? null : (rendered.find((s) => s.index === activeIndex)?.url ?? null);

  // Fetch + parse the active subtitle track (only when the URL actually changes).
  useEffect(() => {
    setText('');
    pointer.current = 0;
    if (!activeUrl) {
      setCues([]);
      return;
    }
    let cancelled = false;
    fetch(activeUrl)
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status)))))
      .then((raw) => {
        if (!cancelled) setCues(parseVtt(raw));
      })
      .catch(() => {
        if (!cancelled) setCues([]);
      });
    return () => {
      cancelled = true;
    };
  }, [activeUrl]);

  // Sync the active cue to the video clock.
  useEffect(() => {
    const v = videoRef.current;
    if (!v || cues.length === 0) {
      setText('');
      return;
    }

    let last = '';
    const update = () => {
      const { text: t, index } = activeCueText(cues, baseSec + v.currentTime, pointer.current);
      pointer.current = index;
      if (t !== last) {
        last = t;
        setText(t);
      }
    };
    // Re-anchor the moving cue pointer after a seek so captions are correct
    // immediately at the new position (`activeCueText` binary-searches when the
    // hint is stale). `seeking` (cancel the now-wrong line) + `seeked` (resync).
    const reanchor = () => {
      pointer.current = 0;
      update();
    };
    v.addEventListener('timeupdate', update);
    v.addEventListener('seeking', reanchor);
    v.addEventListener('seeked', reanchor);
    update();
    return () => {
      v.removeEventListener('timeupdate', update);
      v.removeEventListener('seeking', reanchor);
      v.removeEventListener('seeked', reanchor);
    };
    // `baseSec` changes on a re-anchor, which REMOUNTS the <video>; re-run so the
    // listeners bind to the fresh element and use the new offset.
  }, [videoRef, cues, baseSec]);

  if (!text) return null;

  return (
    <div
      className="pointer-events-none absolute inset-x-0 z-30 px-[8%] text-center transition-[bottom] duration-300"
      style={{ bottom: raised ? '15%' : '7%' }}
    >
      <span style={subtitleCss(style)}>{text}</span>
    </div>
  );
}

/** Memoised: with a stable `rendered` array it won't re-render on the player's
 * ~4×/s timeupdate renders only its own cue-change state drives updates. */
export const SubtitleLayer = memo(SubtitleLayerImpl);
