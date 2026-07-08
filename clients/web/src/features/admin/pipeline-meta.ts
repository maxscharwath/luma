// Shared visual meta for the element-centric pipeline dashboard: status / kind
// color maps + the poster gradient fallback. Pure data (no JSX, no i18n) so the
// row and drawer can share it; labels come from the i18n catalog in the
// components.

export type Meta = { color: string; bg: string; ring: string; dot: string; pulse?: boolean };
export type KindMeta = { color: string; bg: string; typeKey: 'movie' | 'show' | 'episode' };

/** Compact duration label from milliseconds ("1 h 42" / "42 min"; empty if none). */
export function fmtDur(ms?: number | null): string {
  if (!ms) return '';
  const m = Math.round(ms / 60000);
  return m >= 60 ? `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, '0')}` : `${m} min`;
}

const PENDING: Meta = {
  color: 'rgba(244,243,240,.55)',
  bg: 'rgba(255,255,255,.05)',
  ring: 'rgba(255,255,255,.12)',
  dot: 'rgba(244,243,240,.4)',
};

/** Per-treatment status (a stage applied to one element). */
const STATUS_META: Record<string, Meta> = {
  done: { color: '#46D08D', bg: 'rgba(70,208,141,.13)', ring: 'rgba(70,208,141,.4)', dot: '#46D08D' },
  running: { color: '#F4B642', bg: 'rgba(242,180,66,.15)', ring: 'rgba(242,180,66,.5)', dot: '#F4B642', pulse: true },
  failed: { color: '#E8536A', bg: 'rgba(232,83,106,.13)', ring: 'rgba(232,83,106,.45)', dot: '#E8536A' },
  pending: PENDING,
  missing: PENDING,
};
export const statusMeta = (s: string): Meta => STATUS_META[s] ?? PENDING;

/** An element's overall roll-up. */
const OVERALL_PENDING: Meta = { ...PENDING, color: 'rgba(244,243,240,.7)', bg: 'rgba(255,255,255,.06)', dot: 'rgba(244,243,240,.45)' };
const OVERALL_META: Record<string, Meta> = {
  ok: { color: '#46D08D', bg: 'rgba(70,208,141,.13)', ring: 'rgba(70,208,141,.4)', dot: '#46D08D' },
  running: { color: '#F4B642', bg: 'rgba(242,180,66,.14)', ring: 'rgba(242,180,66,.5)', dot: '#F4B642', pulse: true },
  pending: OVERALL_PENDING,
  failed: { color: '#E8536A', bg: 'rgba(232,83,106,.13)', ring: 'rgba(232,83,106,.45)', dot: '#E8536A' },
};
export const overallMeta = (s: string): Meta => OVERALL_META[s] ?? OVERALL_PENDING;

/** Element kind → badge color + the i18n type key for its label. */
const FILM_KIND: KindMeta = { color: '#F4B642', bg: 'rgba(242,180,66,.14)', typeKey: 'movie' };
const KIND_META: Record<string, KindMeta> = {
  film: FILM_KIND,
  series: { color: '#C792EA', bg: 'rgba(199,146,234,.14)', typeKey: 'show' },
  episode: { color: '#86A8FF', bg: 'rgba(134,168,255,.14)', typeKey: 'episode' },
};
export const kindMeta = (k: string): KindMeta => KIND_META[k] ?? FILM_KIND;

/** Deterministic poster gradient from a seed (shown behind / until a real poster
 *  loads), mirroring the design's placeholder. */
export function posterGrad(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) % 360;
  return `radial-gradient(120% 90% at 30% 16%, hsla(${(h + 22) % 360},60%,46%,.5), transparent 62%), linear-gradient(155deg, hsl(${h} 42% 27%), hsl(${(h + 30) % 360} 48% 10%))`;
}
