import { type PointerEvent as ReactPointerEvent, useCallback, useRef, useState } from 'react';
import type { StoryboardTile } from '../storyboard';
import { clamp01 } from './fmt';
import type { Chapter } from './types';

export interface ChapterProgressBarProps {
  cur: number;
  dur: number;
  bufEnd: number;
  /** Pending scrub target while dragging / D-pad seeking (null when settled). */
  seekPreview: number | null;
  /** Normalized chapters; empty = one continuous segment (graceful fallback). */
  chapters: Chapter[];
  /** Storyboard thumbnail at a position (null until the sheet is ready). */
  tileAt: (sec: number) => StoryboardTile | null;
  /** The progress zone is the active D-pad focus (ring + always preview). */
  focused: boolean;
  /** Left label: elapsed time. */
  elapsed: string;
  /** Current chapter title, shown next to the elapsed time (empty to hide). */
  chapterLabel?: string;
  /** Right labels: total runtime + real end clock ("fin à 22h38"). */
  total: string;
  endsAt: string;
  /** Live scrub preview (absolute seconds) while dragging. */
  onScrub: (sec: number) => void;
  onScrubCommit: () => void;
}

/**
 * The chapter-aware progress bar (§1, §2), matching the 10-foot design: an info
 * row (elapsed . current-chapter on the left, runtime . end-clock on the right)
 * above a track of distinct chapter segments, each with its own amber played
 * fill + lighter buffered zone, a playhead pill, and the storyboard preview that
 * follows the cursor (mouse) or the position (D-pad). Pointer down-drag-up
 * previews then commits one seek click-to-point is the zero-length drag.
 */
export function ChapterProgressBar({
  cur,
  dur,
  bufEnd,
  seekPreview,
  chapters,
  tileAt,
  focused,
  elapsed,
  chapterLabel,
  total,
  endsAt,
  onScrub,
  onScrubCommit,
}: Readonly<ChapterProgressBarProps>) {
  const trackRef = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const [hoverSec, setHoverSec] = useState<number | null>(null);

  const shown = seekPreview ?? cur;
  const shownPct = dur > 0 ? clamp01(shown / dur) * 100 : 0;

  const secAt = useCallback(
    (clientX: number): number | null => {
      const el = trackRef.current;
      if (!el || dur <= 0) return null;
      const r = el.getBoundingClientRect();
      return clamp01((clientX - r.left) / r.width) * dur;
    },
    [dur],
  );

  const onDown = useCallback(
    (e: ReactPointerEvent) => {
      if (e.button !== 0) return;
      e.preventDefault();
      const s = secAt(e.clientX);
      if (s == null) return;
      dragging.current = true;
      onScrub(s);
      const move = (ev: PointerEvent) => {
        if (!dragging.current) return;
        const m = secAt(ev.clientX);
        if (m != null) {
          onScrub(m);
          setHoverSec(m);
        }
      };
      const up = () => {
        if (!dragging.current) return;
        dragging.current = false;
        window.removeEventListener('pointermove', move);
        window.removeEventListener('pointerup', up);
        onScrubCommit();
      };
      window.addEventListener('pointermove', move);
      window.addEventListener('pointerup', up);
    },
    [secAt, onScrub, onScrubCommit],
  );

  // Segments: real chapters, or a single implicit chapter over the whole runtime.
  const segs =
    chapters.length > 0
      ? chapters
      : [{ startMs: 0, endMs: dur * 1000, title: '', kind: 'chapter' as const }];
  const shownMs = shown * 1000;
  const bufMs = bufEnd * 1000;

  // Preview follows the cursor on hover, else the position while focused (D-pad).
  let previewSec: number | null = null;
  if (hoverSec != null) previewSec = hoverSec;
  else if (focused) previewSec = shown;
  const previewTile = previewSec != null ? tileAt(previewSec) : null;
  const previewPct = previewSec != null && dur > 0 ? clamp01(previewSec / dur) * 100 : 0;

  return (
    <div className="mb-5">
      {/* info row */}
      <div className="mb-[13px] flex items-baseline justify-between">
        <span className="font-sans text-[18px] font-semibold text-[#F4F3F0] tabular-nums">
          {elapsed}
          {chapterLabel ? (
            <span className="font-medium text-[rgba(244,243,240,0.5)]"> · {chapterLabel}</span>
          ) : null}
        </span>
        <span className="font-sans text-[18px] font-semibold text-[rgba(244,243,240,0.5)] tabular-nums">
          {total}
          {endsAt ? (
            <span className="font-medium text-[rgba(244,243,240,0.38)]"> · {endsAt}</span>
          ) : null}
        </span>
      </div>

      {/* track */}
      <div
        role="slider"
        aria-label="progress"
        aria-valuemin={0}
        aria-valuemax={Math.round(dur)}
        aria-valuenow={Math.round(shown)}
        tabIndex={-1}
        onPointerDown={onDown}
        onPointerMove={(e) => setHoverSec(secAt(e.clientX))}
        onPointerLeave={() => {
          if (!dragging.current) setHoverSec(null);
        }}
        className={`relative flex h-[18px] touch-none cursor-pointer items-center gap-1 rounded-full px-0.5 transition-shadow duration-200 ${
          focused ? 'shadow-[0_0_0_4px_rgba(242,180,66,0.28)]' : ''
        }`}
      >
        {/* storyboard preview + timestamp */}
        {previewSec != null ? (
          <div
            className="pointer-events-none absolute bottom-9 z-6 flex -translate-x-1/2 flex-col items-center gap-2"
            style={{ left: `${previewPct}%` }}
          >
            {previewTile ? (
              <div
                className="relative overflow-hidden rounded-lg border-2 border-[rgba(255,255,255,0.3)] bg-black shadow-[0_16px_40px_rgba(0,0,0,0.7)]"
                style={previewTile as object}
              >
                <div className="absolute inset-0 bg-[radial-gradient(120%_120%_at_50%_35%,transparent,rgba(0,0,0,0.5))]" />
              </div>
            ) : null}
            <span className="rounded-lg bg-[rgba(0,0,0,0.8)] px-3 py-1 font-sans text-[14px] font-bold text-white tabular-nums">
              {fmtSec(previewSec)}
            </span>
          </div>
        ) : null}

        {/* segmented track */}
        <div ref={trackRef} className="relative flex h-1.5 flex-1 items-center gap-1">
          {segs.map((seg) => {
            const span = Math.max(1, seg.endMs - seg.startMs);
            const played = clamp01((shownMs - seg.startMs) / span);
            const buffed = clamp01((bufMs - seg.startMs) / span);
            return (
              <div
                key={seg.startMs}
                className="relative h-1.5 flex-1 overflow-hidden rounded-full bg-[rgba(255,255,255,0.2)]"
              >
                <div
                  className="absolute inset-0 rounded-full bg-[rgba(255,255,255,0.28)]"
                  style={{ right: `${(1 - buffed) * 100}%` }}
                />
                <div
                  className="absolute inset-0 rounded-full bg-[linear-gradient(90deg,#F4B642,#FFD262)]"
                  style={{ right: `${(1 - played) * 100}%` }}
                />
              </div>
            );
          })}

          {/* playhead pill */}
          <div
            className="absolute top-1/2 h-4 w-4 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white shadow-[0_0_0_4px_rgba(242,180,66,0.5),0_2px_8px_rgba(0,0,0,0.6)]"
            style={{ left: `${shownPct}%` }}
          />
        </div>
      </div>
    </div>
  );
}

/** Local mm:ss / h:mm:ss for the preview bubble (avoids importing to keep it terse). */
function fmtSec(s: number): string {
  const t = Math.max(0, Math.floor(s));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const sec = t % 60;
  const mm = h > 0 ? String(m).padStart(2, '0') : String(m);
  const hh = h > 0 ? `${h}:` : '';
  return `${hh}${mm}:${String(sec).padStart(2, '0')}`;
}
