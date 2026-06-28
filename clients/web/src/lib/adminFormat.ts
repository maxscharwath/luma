// Formatting + deterministic-gradient helpers for the admin console. Mirrors the
// helpers baked into the `Admin Serveur` design (poster/avatar gradients, byte +
// duration formatting, French decimal commas).

/** Deterministic hue (0..359) from a string — same as the design's `_hue`. */
export function hue(s: string): number {
  let h = 0;
  for (let i = 0; i < (s || '').length; i++) h = (h * 31 + s.charCodeAt(i)) % 360;
  return h;
}

/** Avatar gradient for a name (matches the design's `avatarGrad`). */
export function avatarGradient(name: string): string {
  const h = hue(name);
  return `linear-gradient(140deg, hsl(${h} 48% 46%), hsl(${(h + 40) % 360} 54% 26%))`;
}

/** Poster gradient for a title (matches the design's `posterGrad`). */
export function posterGradient(title: string): string {
  const h = hue(title);
  return `radial-gradient(120% 90% at 30% 16%, hsla(${(h + 22) % 360},60%,46%,.5), transparent 62%), linear-gradient(155deg, hsl(${h} 42% 27%), hsl(${(h + 30) % 360} 48% 10%))`;
}

/** First letter, upper-cased. */
export function initial(name: string): string {
  return (name?.[0] ?? '?').toUpperCase();
}

/** A French-style decimal (comma) with `digits` places. */
export function decimal(n: number, digits = 1): string {
  return n.toFixed(digits).replace('.', ',');
}

/** Human byte size: To / Go / Mo / Ko. */
export function formatBytes(bytes: number): string {
  if (!bytes || bytes < 0) return '0 o';
  const units = ['o', 'Ko', 'Mo', 'Go', 'To', 'Po'];
  const i = Math.min(units.length - 1, Math.floor(Math.log(bytes) / Math.log(1024)));
  const v = bytes / 1024 ** i;
  return `${decimal(v, v >= 100 || i <= 1 ? 0 : 1)} ${units[i]}`;
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
  if (Number.isNaN(then)) return '—';
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
