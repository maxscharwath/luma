// Small time formatters shared by the jobs page + its detail panel.

/** Locale-aware relative time (uses the browser/Intl locale), e.g. "in 3 hours". */
export function rel(ms: number): string {
  const rtf = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' });
  const diff = ms - Date.now();
  const abs = Math.abs(diff);
  if (abs < 60_000) return rtf.format(Math.round(diff / 1000), 'second');
  if (abs < 3_600_000) return rtf.format(Math.round(diff / 60_000), 'minute');
  if (abs < 86_400_000) return rtf.format(Math.round(diff / 3_600_000), 'hour');
  return rtf.format(Math.round(diff / 86_400_000), 'day');
}

/** Compact duration: `820 ms` / `4.3 s` / `2 min 05 s`. */
export function dur(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  // Round to whole seconds FIRST, then split rounding the remainder separately
  // can yield a stray "X min 60 s" for sub-second tails ≥ 59.5 s.
  const total = Math.round(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m} min ${String(s).padStart(2, '0')} s`;
}

/** Absolute clock time (hh:mm) in the local locale. */
export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}
