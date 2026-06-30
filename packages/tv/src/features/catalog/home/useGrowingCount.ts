import { type RefObject, useEffect, useRef, useState } from 'react';

/**
 * Grow `count` toward `total` as a bottom sentinel nears the viewport. Keeps the
 * DOM bounded while the user is near the top of a long grid, then fills in as they
 * scroll so a 1000-item library never mounts all at once.
 */
export function useGrowingCount(total: number, step: number): [number, RefObject<HTMLDivElement>] {
  const [count, setCount] = useState(() => Math.min(step, total));
  const sentinel = useRef<HTMLDivElement>(null);

  useEffect(() => setCount(Math.min(step, total)), [total, step]);

  useEffect(() => {
    const el = sentinel.current;
    if (!el || count >= total) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) setCount((c) => Math.min(c + step, total));
      },
      { rootMargin: '800px' },
    );
    io.observe(el);
    return () => io.disconnect();
  }, [count, total, step]);

  return [count, sentinel];
}
