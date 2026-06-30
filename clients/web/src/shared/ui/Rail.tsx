import { IconChevronLeft, IconChevronRight, type TablerIcon } from '@tabler/icons-react';
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';

export interface RailProps {
  children: ReactNode;
  /** Gap between slides, in px. */
  gap?: number;
  /** Inset the track by the page gutter (`--gutter-web`) for full-bleed
   * sections (detail rails) where the heading is gutter-padded. */
  padded?: boolean;
  label?: string;
}

const ARROW: Record<'prev' | 'next', TablerIcon> = {
  prev: IconChevronLeft,
  next: IconChevronRight,
};

function Arrow({
  dir,
  show,
  onClick,
}: Readonly<{ dir: 'prev' | 'next'; show: boolean; onClick: () => void }>) {
  const Glyph = ARROW[dir];
  return (
    <button
      type="button"
      tabIndex={-1}
      aria-hidden="true"
      onClick={onClick}
      className={`absolute top-1/2 z-10 hidden h-11 w-11 -translate-y-1/2 items-center justify-center rounded-full
        border border-white/14 bg-[#16161a] text-white shadow-[0_6px_18px_rgba(0,0,0,.5)]
        transition-opacity duration-200 hover:bg-[#202028] md:flex
        ${dir === 'prev' ? 'left-1.5' : 'right-1.5'}
        ${show ? 'opacity-0 group-hover/rail:opacity-100' : 'pointer-events-none opacity-0'}`}
    >
      <Glyph size={22} stroke={2} />
    </button>
  );
}

/**
 * Horizontal scroller **native** overflow scrolling (GPU-composited, off the
 * main thread, so it never janks) with modern JS niceties layered on: the mouse
 * wheel scrolls it horizontally (with edge-release back to the page), hover
 * prev/next arrows page smoothly, and the scrollbar is hidden. `py-3` gives the
 * cards' hover-lift + amber ring room before the overflow clips.
 */
export function Rail({ children, gap = 18, padded = false, label }: Readonly<RailProps>) {
  const ref = useRef<HTMLDivElement>(null);
  const [canPrev, setCanPrev] = useState(false);
  const [canNext, setCanNext] = useState(false);

  const sync = useCallback(() => {
    const el = ref.current;
    if (!el) return;
    const max = el.scrollWidth - el.clientWidth;
    setCanPrev(el.scrollLeft > 4);
    setCanNext(el.scrollLeft < max - 4);
  }, []);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    sync();

    // Mouse wheel → horizontal. Only hijack a vertical-dominant wheel while the
    // rail can still move that way; at the edges we let the page scroll.
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaY) <= Math.abs(e.deltaX)) return; // native handles horizontal
      const max = el.scrollWidth - el.clientWidth;
      if (max <= 0) return;
      const atStart = el.scrollLeft <= 0;
      const atEnd = el.scrollLeft >= max - 1;
      if ((e.deltaY < 0 && atStart) || (e.deltaY > 0 && atEnd)) return; // edge-release
      e.preventDefault();
      el.scrollLeft += e.deltaY;
    };

    el.addEventListener('wheel', onWheel, { passive: false });
    el.addEventListener('scroll', sync, { passive: true });
    const ro = new ResizeObserver(sync);
    ro.observe(el);
    return () => {
      el.removeEventListener('wheel', onWheel);
      el.removeEventListener('scroll', sync);
      ro.disconnect();
    };
  }, [sync]);

  const page = (dir: 1 | -1) => {
    const el = ref.current;
    if (el) el.scrollBy({ left: dir * el.clientWidth * 0.85, behavior: 'smooth' });
  };

  return (
    <div className="group/rail relative">
      <div
        ref={ref}
        aria-label={label}
        // `py-4` + the horizontal inset give the cards' hover-lift + amber focus
        // ring room before the overflow clips them. Non-padded (home) rails inset
        // with `px-4 -mx-4` so the first/last card's ring isn't cropped at the
        // scroll edge while the cards stay aligned with the section heading.
        className={`flex overflow-x-auto py-4 [-ms-overflow-style:none] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden
          ${padded ? 'px-(--gutter-web)' : 'px-4 -mx-4'}`}
        style={{ gap: `${gap}px` }}
      >
        {children}
      </div>
      <Arrow dir="prev" show={canPrev} onClick={() => page(-1)} />
      <Arrow dir="next" show={canNext} onClick={() => page(1)} />
    </div>
  );
}
