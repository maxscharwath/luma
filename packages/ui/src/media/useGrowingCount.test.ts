// @vitest-environment jsdom
import { act, renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { useGrowingCount } from './useGrowingCount';

/** A scroll event positioned `remaining` px from the end of the content. */
function scrollTo(remaining: number) {
  const viewport = 1000;
  const content = 5000;
  return {
    nativeEvent: {
      contentOffset: { x: 0, y: content - viewport - remaining },
      contentSize: { width: 1920, height: content },
      layoutMeasurement: { width: 1920, height: viewport },
    },
  } as never;
}

describe('useGrowingCount', () => {
  it('starts at one chunk, or the whole list when it is shorter', () => {
    expect(renderHook(() => useGrowingCount(1000, 120)).result.current.count).toBe(120);
    expect(renderHook(() => useGrowingCount(30, 120)).result.current.count).toBe(30);
  });

  it('does not grow while the end is still far away', () => {
    const { result } = renderHook(() => useGrowingCount(1000, 120));
    act(() => result.current.onScroll(scrollTo(2000)));
    expect(result.current.count).toBe(120);
  });

  it('adds a chunk when the end comes within the lookahead', () => {
    const { result } = renderHook(() => useGrowingCount(1000, 120));
    act(() => result.current.onScroll(scrollTo(400)));
    expect(result.current.count).toBe(240);
    act(() => result.current.onScroll(scrollTo(0)));
    expect(result.current.count).toBe(360);
  });

  it('never overshoots the total', () => {
    const { result } = renderHook(() => useGrowingCount(150, 120));
    act(() => result.current.onScroll(scrollTo(0)));
    expect(result.current.count).toBe(150);
    act(() => result.current.onScroll(scrollTo(0)));
    expect(result.current.count).toBe(150);
  });

  it('restarts from the first chunk when the list is replaced', () => {
    const { result, rerender } = renderHook(({ total }) => useGrowingCount(total, 120), {
      initialProps: { total: 1000 },
    });
    act(() => result.current.onScroll(scrollTo(0)));
    expect(result.current.count).toBe(240);
    rerender({ total: 42 });
    expect(result.current.count).toBe(42);
  });
});
