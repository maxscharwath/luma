// The load-in / cross-fade state machine, shared by both <Img> renderers.
//
// Platform-neutral on purpose: it holds no element ref and touches no DOM, so
// the native and web renderers differ only in HOW they display a layer, never in
// WHEN. That is what keeps a hero swap looking identical on Apple TV and Tizen.

import { useEffect, useRef, useState } from 'react';

export interface CrossFade {
  /** The current `src` has decoded and can be revealed. */
  loaded: boolean;
  /** The current `src` failed; show the background / fallback instead. */
  errored: boolean;
  /** The previous, still fully loaded image held underneath while the incoming
   *  one decodes. Null once the fade has finished, or when there is nothing to
   *  fade from (first load, or clearing to no art). */
  under: string | null;
  markLoaded: () => void;
  markErrored: () => void;
}

export function useCrossFade(src: string | null, duration: number): CrossFade {
  const [shown, setShown] = useState<string | null>(src);
  const [loaded, setLoaded] = useState(false);
  const [errored, setErrored] = useState(false);
  const [under, setUnder] = useState<string | null>(null);
  const loadedSrc = useRef<string | null>(null);

  // Adjusted during render rather than in an effect: a post-commit update would
  // paint one frame of the new (transparent) image over nothing, which reads as
  // a flicker. Promote the last fully-loaded image to the underlay and start the
  // incoming one at opacity 0. Clearing to null (or to the same url) drops the
  // underlay, so we never cross-fade from stale art.
  if (shown !== src) {
    const prev = loadedSrc.current;
    setUnder(src && prev && prev !== src ? prev : null);
    setShown(src);
    setLoaded(false);
    setErrored(false);
  }

  // Drop the underlay once the incoming image has finished fading in over it.
  useEffect(() => {
    if (!loaded || under == null) return;
    const id = setTimeout(() => setUnder(null), duration);
    return () => clearTimeout(id);
  }, [loaded, under, duration]);

  return {
    loaded,
    errored,
    under,
    markLoaded: () => {
      loadedSrc.current = src;
      setLoaded(true);
    },
    markErrored: () => {
      setErrored(true);
      setUnder(null);
    },
  };
}
