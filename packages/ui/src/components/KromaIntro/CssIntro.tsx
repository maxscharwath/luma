import { useCallback, useEffect, useRef, useState } from 'react';
import { DEFAULT_AUDIO, SAFETY_MS } from './constants';
import { IntroScene } from './IntroScene';
import { IntroShell } from './IntroShell';
import { useIntroExit } from './useIntroExit';
import { useIntroKeys } from './useIntroKeys';

export interface CssIntroProps {
  onDone: () => void;
  loop?: boolean;
  /** Optional tagline under the lockup (none by default). */
  tagline?: string;
  lite?: boolean;
}

/**
 * CSS/DOM fallback intro, used when the video intro cannot play (decode or load
 * failure): a total-black open, an amber glow that ignites the chromatic wheel
 * (segments build clockwise while the wheel spins into place → hub-glow pulse →
 * shockwave), an impact flash + scale punch synced to the 1.30 s bass hit, the
 * "KROMA" wordmark reveal with a metal sheen, then the tagline. Choreographed to
 * the ~5 s audio sting; the visual timeline starts at audio onset (or its
 * rejection) so picture and sound stay locked together, a pointer/key fallback
 * unblocks the sound on the first interaction, and a safety timer guarantees the
 * intro still ends even if audio never plays.
 *
 * Shares its frame, exit hand-off and skip keys with the video intro through
 * {@link IntroShell} / {@link useIntroExit} / {@link useIntroKeys}; only the
 * medium (an audio sting driving CSS layers, rather than a film) differs.
 *
 * `lite` (set by the TV shells) trades a little polish for a smooth frame rate on
 * weak TV GPUs: it drops the per-frame raster work desktop can absorb but a TV
 * can't animated `filter: blur()`, the `mix-blend-mode` grain, the
 * `background-position` sheen and keeps animation on the compositor.
 */
export function CssIntro({ onDone, loop = false, tagline, lite = false }: Readonly<CssIntroProps>) {
  // `started` gates the animated layers so the CSS timeline begins exactly at
  // audio onset. `runId` is the React key that restarts every animation on replay.
  const [started, setStarted] = useState(false);
  const [runId, setRunId] = useState(0);
  const { exiting, safetyRef, exit, reopen, clearTimers } = useIntroExit(onDone);

  const audioRef = useRef<HTMLAudioElement | null>(null);
  const loopRef = useRef(loop);
  loopRef.current = loop;

  const start = useCallback(() => {
    reopen();
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
      if (typeof p?.then === 'function') p.then(begin).catch(begin);
      else begin();
    } else {
      begin();
    }
    if (!loopRef.current) safetyRef.current = setTimeout(exit, SAFETY_MS);
  }, [exit, reopen, safetyRef]);

  const replay = useCallback(() => {
    setRunId((n) => n + 1);
    start();
  }, [start]);

  // First gesture: if the sting never got past the autoplay block, start it now
  // and run the synced timeline from there.
  const unblock = useCallback(() => {
    const a = audioRef.current;
    if (!a?.paused) return;
    try {
      a.currentTime = 0;
    } catch {
      /* harmless */
    }
    void a
      .play()
      .then(() => setStarted(true))
      .catch(() => undefined);
  }, []);

  useIntroKeys({ exit, replay, unblock });

  // biome-ignore lint/correctness/useExhaustiveDependencies: mount-only intro timeline; arm audio once. start/exit/replay are stable useCallbacks and are intentionally omitted so the effect never re-arms (which would restart the sting) on unrelated re-renders.
  useEffect(() => {
    const a = new Audio(DEFAULT_AUDIO);
    a.preload = 'auto';
    audioRef.current = a;

    const onEnded = () => {
      if (loopRef.current) replay();
      else exit();
    };
    a.addEventListener('ended', onEnded);

    start();

    return () => {
      clearTimers();
      a.pause();
      a.removeEventListener('ended', onEnded);
    };
  }, []);

  return (
    <IntroShell exiting={exiting}>
      {started ? (
        <IntroScene
          runId={runId}
          lite={lite}
          showTagline={Boolean(tagline)}
          tagline={tagline ?? ''}
        />
      ) : null}
    </IntroShell>
  );
}
