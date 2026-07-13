// WebVTT parsing + active-cue lookup, shared by every client's custom subtitle
// renderer (web SubtitleLayer, TV TvSubtitles). Cross-origin <track> cues never
// load, so each client fetches the VTT itself and renders cues from these.

// Text subtitle codecs the server can serve as on-demand WebVTT (image subs like
// PGS/VobSub cannot be rendered in a <track>).
const TEXT_SUB_CODECS = new Set(['subrip', 'srt', 'ass', 'ssa', 'mov_text', 'webvtt', 'vtt']);

/** Whether a subtitle codec can be served as WebVTT (i.e. rendered as text). */
export function isTextSubtitle(codec: string): boolean {
  return TEXT_SUB_CODECS.has(codec);
}

export interface Cue {
  start: number;
  end: number;
  text: string;
}

/** Strip WebVTT inline markup (`<i>`, `<c.classname>`, `{...}`) to plain text. */
function clean(text: string): string {
  return text
    .replace(/<[^>]+>/g, '')
    .replace(/\{[^}]+\}/g, '')
    .trim();
}

/** Parse `HH:MM:SS.mmm` / `MM:SS.mmm` (`,` or `.` ms) to seconds. */
function parseTs(ts: string): number {
  const parts = ts.replace(',', '.').split(':').map(Number);
  return parts.reduce((acc, p) => acc * 60 + (Number.isFinite(p) ? p : 0), 0);
}

/** Minimal, fast WebVTT parser → cue list sorted by start time. */
export function parseVtt(raw: string): Cue[] {
  const cues: Cue[] = [];
  for (const block of raw.replace(/\r/g, '').split('\n\n')) {
    const lines = block.split('\n');
    const ti = lines.findIndex((l) => l.includes('-->'));
    if (ti === -1) continue;
    const timing = lines[ti];
    if (timing === undefined) continue;
    const [a, b] = timing.split('-->').map((s) => s.trim().split(/\s+/)[0] ?? '');
    const start = parseTs(a ?? '');
    const end = parseTs(b ?? '');
    if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) continue;
    const text = clean(lines.slice(ti + 1).join('\n'));
    if (text) cues.push({ start, end, text });
  }
  return cues.sort((x, y) => x.start - y.start);
}

/**
 * The active cue's text at time `t`. `hint` is the last returned index an O(1)
 * amortised moving pointer for normal playback (cues advance by one), with a
 * binary search to re-sync after a seek. Returns the text and the new pointer to
 * remember for the next call.
 */
export function activeCueText(
  cues: Cue[],
  t: number,
  hint: number,
): { text: string; index: number } {
  if (cues.length === 0) return { text: '', index: 0 };

  const cur = cues[hint];
  // Fast path: still inside the current cue.
  if (cur && t >= cur.start && t <= cur.end) return { text: cur.text, index: hint };
  // Walk forward a few cues (normal playback advances by one).
  if (cur && t > cur.end) {
    for (let i = hint + 1; i < cues.length && i <= hint + 3; i++) {
      const c = cues[i];
      if (!c) continue;
      if (t < c.start) return { text: '', index: hint };
      if (t <= c.end) return { text: c.text, index: i };
    }
  }
  // Binary search (covers seeks / large jumps).
  let lo = 0;
  let hi = cues.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const c = cues[mid];
    if (!c) break;
    if (t < c.start) hi = mid - 1;
    else if (t > c.end) lo = mid + 1;
    else return { text: c.text, index: mid };
  }
  return { text: '', index: Math.max(0, lo - 1) };
}

import type { MessageKey } from './i18n';

/** Message key for a generation `stage` (see the server's GenRegistry stages). */
export function subtitleStageKey(stage: string): MessageKey {
  switch (stage) {
    case 'model':
      return 'player.subStageModel';
    case 'extract':
      return 'player.subStageExtract';
    case 'transcribe':
      return 'player.subStageTranscribe';
    case 'translate':
      return 'player.subStageTranslate';
    case 'error':
      return 'player.subStageError';
    default:
      return 'player.subStageQueued';
  }
}

/** Human, compact remaining time for a generation ETA ("1 min" / "20 s"). */
export function subtitleEtaTime(sec: number): string {
  return sec >= 60 ? `${Math.round(sec / 60)} min` : `${Math.max(1, Math.round(sec))} s`;
}
