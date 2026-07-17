import { type RefObject, useCallback, useEffect, useState } from 'react';

export type BoostLevel = 'off' | 'low' | 'med' | 'high';

/** Pre-limiter gain per level (+50 % / +100 % / +200 %). */
export const BOOST_GAIN: Record<Exclude<BoostLevel, 'off'>, number> = {
  low: 1.5,
  med: 2,
  high: 3,
};

const KEY = 'kroma.audioBoost';

// One page-wide AudioContext, created on first enable (a user gesture, so it is
// never born suspended by autoplay policy) and kept for the tab's lifetime -
// browsers cap live contexts, so never one per player mount.
let sharedCtx: AudioContext | null = null;
function audioCtx(): AudioContext | null {
  if (typeof AudioContext === 'undefined') return null;
  if (!sharedCtx) {
    sharedCtx = new AudioContext();
    // A persisted boost hydrates WITHOUT a user gesture, so the context can be
    // born suspended - and an element routed into a suspended context is MUTED.
    // Any interaction un-sticks it (a no-op once running, so keep it forever).
    const resume = () => {
      if (sharedCtx?.state === 'suspended') void sharedCtx.resume();
    };
    document.addEventListener('pointerdown', resume, true);
    document.addEventListener('keydown', resume, true);
  }
  if (sharedCtx.state === 'suspended') void sharedCtx.resume();
  return sharedCtx;
}

interface BoostGraph {
  source: MediaElementAudioSourceNode;
  gain: GainNode;
}

// `createMediaElementSource` is once-per-element for the element's LIFETIME (a
// second call throws), and the player REMOUNTS its <video> on re-anchor / audio
// switch - so graphs are keyed by element, not by player instance.
const graphs = new WeakMap<HTMLMediaElement, BoostGraph>();

/** Route (or re-route) an element's audio for the given level. Off with no
 * existing graph is a no-op: the element keeps its native output path and Web
 * Audio is never involved. Once a graph exists the element's audio ALWAYS flows
 * through it, so "off" becomes a straight source→destination wire. */
function wire(el: HTMLMediaElement, level: BoostLevel): void {
  if (level === 'off' && !graphs.has(el)) return;
  const ctx = audioCtx();
  if (!ctx) return;

  let g = graphs.get(el);
  if (!g) {
    const source = ctx.createMediaElementSource(el);
    const gain = ctx.createGain();
    // A brick-wall-ish limiter after the gain: dialogue rides up with the boost
    // while peaks (explosions, music stings) get squashed instead of clipping.
    const limiter = ctx.createDynamicsCompressor();
    limiter.threshold.value = -6;
    limiter.knee.value = 4;
    limiter.ratio.value = 12;
    limiter.attack.value = 0.003;
    limiter.release.value = 0.25;
    gain.connect(limiter);
    limiter.connect(ctx.destination);
    g = { source, gain };
    graphs.set(el, g);
  }

  g.source.disconnect();
  if (level === 'off') {
    g.source.connect(ctx.destination);
  } else {
    g.gain.gain.value = BOOST_GAIN[level];
    g.source.connect(g.gain);
  }
}

/**
 * Client-side volume boost ("night mode"): a Web Audio gain + limiter behind the
 * player's <video>, so it works on EVERY playback mode (direct play included)
 * without forcing a server transcode. Persisted like the subtitle style.
 *
 * `remountKey` must change whenever the parent remounts the <video> (anchor /
 * audio track), so the graph re-attaches to the fresh element.
 */
export function useAudioBoost(
  videoRef: RefObject<HTMLVideoElement | null>,
  remountKey: string,
): { boost: BoostLevel; setBoost: (b: BoostLevel) => void; supported: boolean } {
  const [boost, setBoostState] = useState<BoostLevel>('off');
  const [supported, setSupported] = useState(false);

  useEffect(() => {
    setSupported(typeof AudioContext !== 'undefined');
    try {
      const raw = localStorage.getItem(KEY);
      if (raw === 'low' || raw === 'med' || raw === 'high') setBoostState(raw);
    } catch {
      /* ignore */
    }
  }, []);

  // Re-wire on level change AND on <video> remount (fresh element, fresh graph).
  // biome-ignore lint/correctness/useExhaustiveDependencies: remountKey tracks the element identity.
  useEffect(() => {
    const v = videoRef.current;
    if (v) wire(v, boost);
  }, [boost, remountKey, videoRef]);

  const setBoost = useCallback((b: BoostLevel) => {
    setBoostState(b);
    try {
      localStorage.setItem(KEY, b);
    } catch {
      /* ignore */
    }
  }, []);

  return { boost, setBoost, supported };
}
