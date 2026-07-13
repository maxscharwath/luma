import { useCallback, useEffect, useRef, useState } from 'react';
import { DEFAULT_AUDIO, EXIT_MS, KEYFRAMES, SAFETY_MS } from './constants';
import { IntroScene } from './IntroScene';

/**
 * LUMA cinematic brand intro ported from the Claude Design source
 * ("LUMA Intro.dc.html"): a total-black open, an amber glow that ignites an
 * aperture mark (ring draw → orbit glint → centre-dot ignite → shockwave), an
 * impact flash + scale punch synced to the 1.30 s bass hit, the "LUMA" wordmark
 * reveal with a metal sheen, then the tagline. Drifting embers and a vignette
 * sit on top.
 *
 * The whole timeline is choreographed to a ~5 s audio sting (bundled here, shared
 * by every client). Because browsers block autoplay-with-sound until a user
 * gesture, the visual timeline only *starts* once `audio.play()` resolves (or is
 * rejected) so picture and sound stay locked together; a pointer/key fallback
 * unblocks the sound on the first interaction, and a safety timer guarantees the
 * intro still ends even if audio never plays. There are no on-screen controls
 * it auto-ends with the sting, and any key / remote button (OK, Back, Space) skips.
 *
 * It is intentionally framework-free plain inline styles + an injected
 * `<style>` of @keyframes, no Tailwind so it renders identically on the web
 * SSR shell and on old TV webviews. Mount it as a full-screen overlay and call
 * `onDone` to hand off to the app.
 *
 * `lite` (set by the TV shells) trades a little polish for a smooth frame rate on
 * weak TV GPUs: it drops the per-frame raster work desktop can absorb but a TV
 * can't animated `filter: blur()`, the `mix-blend-mode` grain, the
 * `background-position` sheen and keeps animation on the compositor (opacity +
 * transform only, big layers promoted with `translateZ`).
 */
export interface LumaIntroProps {
  /** Called once the intro has finished (audio ended or skipped). */
  onDone: () => void;
  /** Audio sting URL. Defaults to the bundled LUMA sting. */
  audioSrc?: string;
  /** Loop forever instead of ending (preview/idle-screen use). */
  loop?: boolean;
  /** Show the "Votre médiathèque, en grand" tagline. */
  showTagline?: boolean;
  /** Override the tagline copy. */
  tagline?: string;
  /** Performance mode for weak TV GPUs compositor-only animation, no blur/blend
   * raster. Visually near-identical; much smoother on a TV webview. */
  lite?: boolean;
}

export function LumaIntro({
  onDone,
  audioSrc = DEFAULT_AUDIO,
  loop = false,
  showTagline = true,
  tagline = 'Votre médiathèque, en grand',
  lite = false,
}: Readonly<LumaIntroProps>) {
  // `started` gates the animated layers so the CSS timeline begins exactly at
  // audio onset. `runId` is the React key that restarts every animation on replay.
  const [started, setStarted] = useState(false);
  const [exiting, setExiting] = useState(false);
  const [runId, setRunId] = useState(0);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const safetyRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const exitRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const exitedRef = useRef(false);
  const loopRef = useRef(loop);
  loopRef.current = loop;
  // Latest onDone without re-running the mount effect (avoids re-arming audio).
  const onDoneRef = useRef(onDone);
  onDoneRef.current = onDone;

  const exit = useCallback(() => {
    if (exitedRef.current) return;
    exitedRef.current = true;
    clearTimeout(safetyRef.current);
    setExiting(true);
    exitRef.current = setTimeout(() => onDoneRef.current(), EXIT_MS);
  }, []);

  const start = useCallback(() => {
    exitedRef.current = false;
    clearTimeout(safetyRef.current);
    clearTimeout(exitRef.current);
    setExiting(false);
    setStarted(false);
    const a = audioRef.current;
    // Kick the visual timeline at audio onset so the flash/punch land on the
    // 1.30 s bass hit (the keyframe delays are timed to the sting).
    const begin = () => setStarted(true);
    if (a) {
      try {
        a.currentTime = 0;
      } catch {
        /* not yet seekable harmless */
      }
      const p = a.play();
      if (p && typeof p.then === 'function') p.then(begin).catch(begin);
      else begin();
    } else {
      begin();
    }
    if (!loopRef.current) safetyRef.current = setTimeout(exit, SAFETY_MS);
  }, [exit]);

  const replay = useCallback(() => {
    clearTimeout(safetyRef.current);
    clearTimeout(exitRef.current);
    setRunId((n) => n + 1);
    start();
  }, [start]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: mount-only intro timeline; arm audio once per `audioSrc`. start/exit/replay are stable useCallbacks and are intentionally omitted so the effect never re-arms (which would restart the sting) on unrelated re-renders.
  useEffect(() => {
    const a = new Audio(audioSrc);
    a.preload = 'auto';
    audioRef.current = a;

    const onEnded = () => {
      if (loopRef.current) replay();
      else exit();
    };
    a.addEventListener('ended', onEnded);

    // Browsers block autoplay-with-sound until a gesture: arm a one-shot
    // unblock on the first pointer/key, then run the synced timeline.
    const unblock = () => {
      if (a.paused) {
        try {
          a.currentTime = 0;
        } catch {
          /* harmless */
        }
        void a
          .play()
          .then(() => setStarted(true))
          .catch(() => undefined);
      }
    };
    document.addEventListener('pointerdown', unblock);
    document.addEventListener('keydown', unblock);

    // Skip / replay via keyboard + TV remote (OK/Enter, Space, Back/Escape).
    const onKey = (e: KeyboardEvent) => {
      const k = e.key;
      if (
        k === 'Enter' ||
        k === ' ' ||
        k === 'Spacebar' ||
        k === 'Escape' ||
        k === 'GoBack' ||
        k === 'BrowserBack'
      ) {
        e.preventDefault();
        e.stopImmediatePropagation();
        exit();
      } else if (k === 'r' || k === 'R') {
        e.preventDefault();
        e.stopImmediatePropagation();
        replay();
      }
    };
    // Capture phase so the TV's spatial focus-nav underneath stays inert.
    window.addEventListener('keydown', onKey, true);

    start();

    return () => {
      clearTimeout(safetyRef.current);
      clearTimeout(exitRef.current);
      a.pause();
      a.removeEventListener('ended', onEnded);
      document.removeEventListener('pointerdown', unblock);
      document.removeEventListener('keydown', unblock);
      window.removeEventListener('keydown', onKey, true);
    };
    // Re-arm only if the audio source changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [audioSrc]);

  return (
    <div
      className="luma-intro"
      style={{
        position: 'fixed',
        inset: 0,
        zIndex: 9999,
        overflow: 'hidden',
        background: '#0A0A0C',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontFamily: "'Hanken Grotesk', system-ui, sans-serif",
      }}
      role="img"
      aria-label="LUMA"
    >
      <style>{KEYFRAMES}</style>

      {started ? (
        <IntroScene runId={runId} lite={lite} showTagline={showTagline} tagline={tagline} />
      ) : null}

      {/* exit transition to the app */}
      <div
        style={{
          position: 'absolute',
          inset: 0,
          background: '#0A0A0C',
          opacity: exiting ? 1 : 0,
          transition: 'opacity .8s ease',
          pointerEvents: 'none',
          zIndex: 50,
        }}
      />
    </div>
  );
}
