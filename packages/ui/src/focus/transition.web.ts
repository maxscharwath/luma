// Focus transition, web (Tizen / webOS / desktop / browser).
//
// A real CSS transition rather than Animated: on a TV's weak CPU a JS-driven
// interpolation of 20 visible rail tiles is a frame-rate cliff, while
// `transition: transform` stays on the compositor. react-native-web passes the
// transition* style props straight through to CSS.
//
// This mirrors the `[data-focus] { transition: ... }` rule the pre-uikit tv.css
// applied globally, now colocated with the component that needs it.

import { motion } from '../tokens';

const DURATION = `${motion.duration.base}ms`;
const TIMING = `cubic-bezier(${motion.bezier.out.join(', ')})`;

/** A focusable with no scale gets no `transform` at all: a browse grid holds
 * hundreds of tiles, and a transform on each promotes each one to its own
 * compositing layer, which a TV GPU pays for even when the value is 1. */
const RING_ONLY = {
  transitionProperty: 'box-shadow, background-color',
  transitionDuration: DURATION,
  transitionTimingFunction: TIMING,
} as const;

export function useFocusScale(focused: boolean, to: number): Record<string, unknown> {
  if (to === 1) return RING_ONLY;
  return {
    transform: [{ scale: focused ? to : 1 }],
    transitionProperty: 'transform, box-shadow, background-color',
    transitionDuration: DURATION,
    transitionTimingFunction: TIMING,
  };
}
