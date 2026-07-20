import { useEffect, useRef, useState } from 'react';
import { type SubtitleAppearance, subtitleCss } from './subtitle-appearance';
import type { PlayerSub } from './types';

/** A parsed WebVTT cue at absolute playback seconds. */
interface Cue {
  startSec: number;
  endSec: number;
  text: string;
}

const TAG = /<[^>]+>/g;

/** Parse an `HH:MM:SS.mmm` or `MM:SS.mmm` timestamp into seconds. */
function toSeconds(v: string): number {
  let sec = 0;
  for (const part of v.trim().split(':'))
    sec = sec * 60 + Number.parseFloat(part.replace(',', '.'));
  return sec;
}

/**
 * Minimal, robust WebVTT parser. Splits on blank lines, reads each block's
 * `start --> end` line (ignoring the `WEBVTT` header, `NOTE` blocks and optional
 * cue-id lines), joins the remaining lines as the cue text and strips simple
 * inline tags. Times are absolute seconds.
 */
function parseVtt(raw: string): Cue[] {
  const cues: Cue[] = [];
  for (const block of raw.replace(/\r\n?/g, '\n').split('\n\n')) {
    const lines = block.split('\n').filter((l) => l.trim() !== '');
    const arrow = lines.findIndex((l) => l.includes('-->'));
    const arrowLine = arrow === -1 ? undefined : lines[arrow];
    if (!arrowLine) continue;
    const [a, rest] = arrowLine.split('-->');
    if (!a || !rest) continue;
    const startSec = toSeconds(a);
    // Drop any cue settings (e.g. `line:90%`) that follow the end timestamp.
    const endTok = rest.trim().split(/\s+/)[0];
    if (!endTok) continue;
    const endSec = toSeconds(endTok);
    if (!Number.isFinite(startSec) || !Number.isFinite(endSec)) continue;
    const text = lines
      .slice(arrow + 1)
      .join('\n')
      .replace(TAG, '')
      .trim();
    if (text) cues.push({ startSec, endSec, text });
  }
  return cues;
}

/** Active-cue lookup with a forward-moving hint (cues are monotonic), falling
 * back to a scan from 0 after a backward jump. */
function cueAt(cues: Cue[], t: number, from: number): { text: string; next: number } {
  let i = Math.max(0, Math.min(from, cues.length - 1));
  const start = cues[i];
  if (start && start.startSec > t) i = 0;
  while (i < cues.length) {
    const ci = cues[i];
    if (!ci || ci.endSec > t) break;
    i += 1;
  }
  const c = cues[i];
  if (c && c.startSec <= t && t < c.endSec) return { text: c.text, next: i };
  return { text: '', next: i };
}

const now = (): number => (typeof performance !== 'undefined' ? performance.now() : Date.now());

interface SubtitleRendererProps {
  /** Absolute playback position in seconds. */
  positionSec: number;
  playing: boolean;
  /** Bumped on every seek so the renderer can re-sync its cue search. */
  seekNonce?: number;
  subtitles: PlayerSub[];
  activeIndex: number | null;
  appearance: SubtitleAppearance;
  /** When true the controls are visible, so lift the caption above them. */
  raised: boolean;
}

/**
 * One position-driven subtitle renderer for web AND TV. It fetches the active
 * track's WebVTT itself (cross-origin `<track>` cues never load when the app and
 * media server differ in origin), caches the parsed cues per url, and renders the
 * cue that contains the current position. The position is interpolated locally
 * while playing so coarse engine clocks (AVPlay/timeupdate) stay in sync.
 */
export function SubtitleRenderer({
  positionSec,
  playing,
  seekNonce,
  subtitles,
  activeIndex,
  appearance,
  raised,
}: Readonly<SubtitleRendererProps>) {
  const [text, setText] = useState('');
  const [cues, setCues] = useState<Cue[]>([]);
  const cache = useRef<Map<string, Cue[]>>(new Map());
  const pointer = useRef(0);

  const activeUrl =
    activeIndex == null ? null : (subtitles.find((s) => s.index === activeIndex)?.url ?? null);

  // Fetch + parse the active track only when its url changes; a per-url ref cache
  // makes switching back to a previously-loaded track instant. Errors → nothing.
  useEffect(() => {
    pointer.current = 0;
    if (!activeUrl) {
      setCues([]);
      return;
    }
    const cached = cache.current.get(activeUrl);
    if (cached) {
      setCues(cached);
      return;
    }
    let cancelled = false;
    fetch(activeUrl)
      .then((r) => (r.ok ? r.text() : Promise.reject(new Error(String(r.status)))))
      .then((raw) => {
        const parsed = parseVtt(raw);
        cache.current.set(activeUrl, parsed);
        if (!cancelled) setCues(parsed);
      })
      .catch(() => {
        if (!cancelled) setCues([]);
      });
    return () => {
      cancelled = true;
    };
  }, [activeUrl]);

  // Re-anchor the moving cue pointer on every committed seek.
  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only on the seek signal.
  useEffect(() => {
    pointer.current = 0;
  }, [seekNonce]);

  // Remember (position, wall-clock) at each report so the cue clock can advance
  // locally between coarse engine updates.
  const clock = useRef({ pos: 0, at: 0 });
  useEffect(() => {
    clock.current = { pos: positionSec, at: now() };
  }, [positionSec]);

  // Tick the interpolated clock and update only when the visible line changes.
  useEffect(() => {
    if (cues.length === 0) {
      setText('');
      return;
    }
    let last: string | null = null;
    const tick = () => {
      const c = clock.current;
      const est = playing ? c.pos + (now() - c.at) / 1000 : c.pos;
      const { text: cur, next } = cueAt(cues, est, pointer.current);
      pointer.current = next;
      if (cur !== last) {
        last = cur;
        setText(cur);
      }
    };
    tick();
    const id = setInterval(tick, 120);
    return () => clearInterval(id);
  }, [cues, playing]);

  if (activeIndex == null || !text) return null;

  // Positioned via inline `bottom` (a runtime value): 17% clears the control
  // chrome when it is up, 9% hugs the frame otherwise. `pre-line` (from
  // subtitleCss) keeps hard line breaks in multi-line cues.
  return (
    <div
      className="pointer-events-none absolute left-0 right-0 z-3 flex flex-col items-center gap-[7px] px-[8%] text-center transition-[bottom] duration-300"
      style={{ bottom: raised ? '17%' : '9%' }}
    >
      <span style={subtitleCss(appearance)}>{text}</span>
    </div>
  );
}
