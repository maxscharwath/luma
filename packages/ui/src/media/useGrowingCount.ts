// Grow a rendered count toward a total as the user approaches the end of a
// scroller, so a 1000-item library never mounts all at once.
//
// Deliberately NOT virtualisation. A FlatList unmounts off-screen rows, and the
// web spatial navigator can only find focusables that are mounted: virtualising
// a browse grid would make the D-pad stop dead at the edge of the viewport. This
// keeps every tile that has been reached in the tree and simply defers the rest.
//
// The trigger is a scroll position rather than an IntersectionObserver sentinel,
// because there is no IntersectionObserver on Apple TV or Android TV. React
// Native's onScroll carries the same information on every platform.

import { useCallback, useEffect, useState } from 'react';
import type { NativeScrollEvent, NativeSyntheticEvent } from 'react-native';

/** How close to the end (in px) starts the next chunk. Generous, because a TV
 * scrolls a whole row at a time and must not wait for a render mid-move. */
const LOOKAHEAD = 800;

export interface GrowingCount {
  /** How many items to render right now. */
  count: number;
  /** Spread onto the ScrollView that owns the list. */
  onScroll: (e: NativeSyntheticEvent<NativeScrollEvent>) => void;
  /** How often onScroll fires, in ms. Also spread onto the ScrollView. */
  scrollEventThrottle: number;
}

export function useGrowingCount(total: number, step: number): GrowingCount {
  const [count, setCount] = useState(() => Math.min(step, total));

  // A new list (a different genre, a new search) restarts from the first chunk.
  useEffect(() => setCount(Math.min(step, total)), [total, step]);

  const onScroll = useCallback(
    (e: NativeSyntheticEvent<NativeScrollEvent>) => {
      const { contentOffset, contentSize, layoutMeasurement } = e.nativeEvent;
      const remaining = contentSize.height - layoutMeasurement.height - contentOffset.y;
      if (remaining > LOOKAHEAD) return;
      setCount((c) => (c >= total ? c : Math.min(c + step, total)));
    },
    [total, step],
  );

  return { count, onScroll, scrollEventThrottle: 100 };
}
