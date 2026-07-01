import { activeCueText, type Cue, parseVtt } from '@luma/core';
import { useT } from '@luma/ui';
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
  playing,
  seekNonce,
  rendered,
  activeIndex,
  raised,
}: Readonly<{
  /** Absolute playback position (s), from the engine no element coupling. */
  positionSec: number;
  /** Whether playback is advancing (drives the interpolated cue clock). */
  playing: boolean;
  /** Bumps on every committed seek so the cue pointer re-anchors. */
  seekNonce: number;
  rendered: { index: number; url: string | null }[];
  activeIndex: number | null;
  raised: boolean;
}>) {
  const t = useT();
  const [cues, setCues] = useState<Cue[]>([]);
  const [text, setText] = useState('');
  // A first-ever embedded track is extracted server-side (a whole-file demux);
  // surface that wait instead of a blank screen. Delayed so a cached track (the
  // common case) resolves without flashing the hint.
  const [loading, setLoading] = useState(false);
  const [showLoading, setShowLoading] = useState(false);
  const pointer = useRef(0);

  const activeUrl =
    activeIndex == null ? null : (rendered.find((s) => s.index === activeIndex)?.url ?? null);

  // Fetch + parse the active subtitle track (only when the URL actually changes).
  useEffect(() => {
    setText('');
    pointer.current = 0;
    if (!activeUrl) {
      setCues([]);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setLoading(true);
    fetch(activeUrl)
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status)))))
      .then((raw) => {
        if (!cancelled) setCues(parseVtt(raw));
      })
      .catch(() => {
        if (!cancelled) setCues([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeUrl]);

  // Reveal the "loading" hint only if the fetch outlasts a short grace period.
  useEffect(() => {
    if (!loading) {
      setShowLoading(false);
      return;
    }
    const id = setTimeout(() => setShowLoading(true), 400);
    return () => clearTimeout(id);
  }, [loading]);

  // Re-anchor the moving cue pointer after a seek so captions match the new
  // position immediately (`activeCueText` binary-searches from a reset hint).
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-anchor only on the seek signal.
  useEffect(() => {
    pointer.current = 0;
  }, [seekNonce]);

  // Interpolated cue clock. The engine reports its position COARSELY (AVPlay's
  // oncurrentplaytime can be a full second apart; timeupdate ~250ms), and cueing
  // straight off those events made lines appear visibly late "not aligned".
  // So: remember (position, wall-clock) at each report and, while playing,
  // advance the estimate locally on a fast tick. Re-render only on text change.
  const clock = useRef({ pos: 0, at: 0 });
  useEffect(() => {
    clock.current = {
      pos: positionSec,
      at: typeof performance !== 'undefined' ? performance.now() : Date.now(),
    };
  }, [positionSec]);

  useEffect(() => {
    if (cues.length === 0) {
      setText('');
      return;
    }
    let last: string | null = null;
    const tick = () => {
      const now = typeof performance !== 'undefined' ? performance.now() : Date.now();
      const c = clock.current;
      const est = playing ? c.pos + (now - c.at) / 1000 : c.pos;
      const { text: t, index } = activeCueText(cues, est, pointer.current);
      pointer.current = index;
      if (t !== last) {
        last = t;
        setText(t);
      }
    };
    tick();
    const id = setInterval(tick, 120);
    return () => clearInterval(id);
  }, [cues, playing]);

  if (!text) {
    if (!showLoading) return null;
    return (
      <div
        className="pointer-events-none absolute inset-x-0 z-30 flex justify-center px-[8%] transition-[bottom] duration-300"
        style={{ bottom: raised ? '24%' : '9%' }}
      >
        <span className="animate-pulse rounded-full bg-black/60 px-5 py-2 text-[26px] font-semibold text-white/85">
          {t('player.subtitleLoading')}
        </span>
      </div>
    );
  }

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
