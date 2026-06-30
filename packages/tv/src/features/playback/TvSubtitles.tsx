import { activeCueText, type Cue, parseVtt } from '@luma/core';
import { type CSSProperties, memo, useEffect, useRef, useState } from 'react';

// 10-foot subtitle styling: large, white, heavy drop-shadow for legibility over
// any artwork. Fixed (no per-user controls on TV).
const TV_SUB_CSS: CSSProperties = {
  color: '#fff',
  fontSize: 'clamp(30px, 3.6vh, 46px)',
  fontWeight: 600,
  lineHeight: 1.3,
  fontFamily: "'Hanken Grotesk', system-ui, sans-serif",
  whiteSpace: 'pre-line',
  display: 'inline-block',
  textShadow: '0 2px 10px rgba(0,0,0,.92), 0 0 3px rgba(0,0,0,.95)',
};

/**
 * Custom subtitle renderer for the TV player. Fetches the active track's WebVTT
 * itself (cross-origin `<track>` elements never load their cues the app and the
 * media server are different origins), parses it (`parseVtt`), and renders the
 * active cue synced to playback (`activeCueText`). Raises above the control bar
 * when the controls are visible so subtitles are never hidden behind them.
 */
function TvSubtitlesImpl({
  positionSec,
  seekNonce,
  rendered,
  activeIndex,
  raised,
}: Readonly<{
  /** Absolute playback position (s), from the engine no element coupling. */
  positionSec: number;
  /** Bumps on every committed seek so the cue pointer re-anchors. */
  seekNonce: number;
  rendered: { index: number; url: string | null }[];
  activeIndex: number | null;
  raised: boolean;
}>) {
  const [cues, setCues] = useState<Cue[]>([]);
  const [text, setText] = useState('');
  const pointer = useRef(0);

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

  // Re-anchor the moving cue pointer after a seek so captions match the new
  // position immediately (`activeCueText` binary-searches from a reset hint).
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-anchor only on the seek signal.
  useEffect(() => {
    pointer.current = 0;
  }, [seekNonce]);

  // Sync the active cue to the absolute playback clock (driven by the engine's
  // position, updated ~4x/s no `<video>` element needed, so AVPlay works too).
  useEffect(() => {
    if (cues.length === 0) {
      setText('');
      return;
    }
    const { text: t, index } = activeCueText(cues, positionSec, pointer.current);
    pointer.current = index;
    setText(t);
  }, [cues, positionSec]);

  if (!text) return null;

  return (
    <div
      className="pointer-events-none absolute inset-x-0 z-30 px-[8%] text-center transition-[bottom] duration-300"
      style={{ bottom: raised ? '24%' : '9%' }}
    >
      <span style={TV_SUB_CSS}>{text}</span>
    </div>
  );
}

export const TvSubtitles = memo(TvSubtitlesImpl);
