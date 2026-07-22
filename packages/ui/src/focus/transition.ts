// Focus transition, native (Apple TV / Android TV).
//
// Driven by Animated with `useNativeDriver`, so the scale runs on the UI thread
// and never crosses the bridge: the same compositor-only guarantee the web tier
// gets from a CSS transition. See transition.web.ts for the web half.

import { useEffect, useRef } from 'react';
import { Animated, Easing } from 'react-native';
import { motion } from '../tokens';

const EASE = Easing.bezier(...(motion.bezier.out as [number, number, number, number]));

/** Nothing to animate: a ring-only focusable adds no style node at all. */
const RING_ONLY: Record<string, unknown> = {};

/** An animated `transform: scale()` that eases to `to` while focused. */
export function useFocusScale(focused: boolean, to: number): Record<string, unknown> {
  const value = useRef(new Animated.Value(1)).current;

  useEffect(() => {
    // A focusable with no scale (to === 1) never animates; skip the work
    // entirely rather than running a no-op timing on every rail tile.
    if (to === 1) return;
    const anim = Animated.timing(value, {
      toValue: focused ? to : 1,
      duration: motion.duration.base,
      easing: EASE,
      useNativeDriver: true,
    });
    anim.start();
    return () => anim.stop();
  }, [focused, to, value]);

  return to === 1 ? RING_ONLY : { transform: [{ scale: value }] };
}
