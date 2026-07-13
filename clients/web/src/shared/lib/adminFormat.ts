// Formatting + deterministic-gradient helpers for the admin console. The core
// hue / decimal / formatBytes helpers live in @luma/admin-kit; this module
// re-exports the ones the web app consumes and keeps the web-specific extras
// (poster gradient, French durations/uptime, relative timestamps) below.

import { decimal, hue } from '@luma/admin-kit';

export { decimal, formatBytes } from '@luma/admin-kit';

/** Poster gradient for a title (matches the design's `posterGrad`). */
export function posterGradient(title: string): string {
  const h = hue(title);
  return `radial-gradient(120% 90% at 30% 16%, hsla(${(h + 22) % 360},60%,46%,.5), transparent 62%), linear-gradient(155deg, hsl(${h} 42% 27%), hsl(${(h + 30) % 360} 48% 10%))`;
}

/** Watch time from milliseconds: "4 h 29 min" / "65 min" / "0 min". */
export function formatDuration(ms: number): string {
  const totalMin = Math.round((ms || 0) / 60000);
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h > 0) return `${h} h ${String(m).padStart(2, '0')} min`;
  return `${m} min`;
}

/** Hours with one decimal (chart axis labels): "14,3 h". */
export function formatHours(ms: number): string {
  return `${decimal((ms || 0) / 3_600_000, 1)} h`;
}

/** Player timecode from ms: "1:42:08" or "8:30". */
export function timecode(ms: number): string {
  const s = Math.max(0, Math.floor((ms || 0) / 1000));
  const hh = Math.floor(s / 3600);
  const mm = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  const p = (n: number) => String(n).padStart(2, '0');
  return hh > 0 ? `${hh}:${p(mm)}:${p(ss)}` : `${mm}:${p(ss)}`;
}

/** Mb/s with a French decimal comma. */
export function formatMbps(n: number): string {
  return decimal(n || 0, 1);
}

/** Uptime "18 j 04 h" / "4 h 12 min" / "8 min". */
export function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return `${d} j ${String(h).padStart(2, '0')} h`;
  if (h > 0) return `${h} h ${String(m).padStart(2, '0')} min`;
  return `${m} min`;
}

/** "il y a 2 h" / "hier" / "à l'instant" from an ISO timestamp (or null). */
export function relativeSeen(iso: string | null | undefined): string {
  if (!iso) return 'jamais';
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return '-';
  const diff = Date.now() - then;
  const min = Math.floor(diff / 60000);
  if (min < 1) return "à l'instant";
  if (min < 60) return `il y a ${min} min`;
  const h = Math.floor(min / 60);
  if (h < 24) return `il y a ${h} h`;
  const d = Math.floor(h / 24);
  if (d === 1) return 'hier';
  if (d < 30) return `il y a ${d} j`;
  return new Date(then).toLocaleDateString('fr-FR');
}
