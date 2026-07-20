// Shared air-date formatting for the requests feature ("Manquants" and
// "Bientôt disponible"), so both pages speak the same date vocabulary. Pure
// functions (no JSX); every date-relative helper takes an optional `now` so
// tests are deterministic.

/** Whole days from today (local midnight) to a `YYYY-MM-DD` date; negative for
 * a past date, 0 for today. */
export function daysFromToday(airDate: string, now: Date = new Date()): number {
  const d = new Date(`${airDate}T00:00:00`);
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  return Math.round((d.getTime() - today.getTime()) / 86_400_000);
}

/** Locale-aware relative air date that scales the unit (days → months → years),
 * like Sonarr's "2 months ago" / "in 3 months", for past AND future dates.
 * Empty for an undated value. */
export function relativeAirDate(
  airDate: string | null,
  locale: string,
  now: Date = new Date(),
): string {
  if (!airDate) return '';
  const days = daysFromToday(airDate, now);
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  if (Math.abs(days) < 31) return rtf.format(days, 'day');
  const months = Math.round(days / 30);
  if (Math.abs(months) < 12) return rtf.format(months, 'month');
  return rtf.format(Math.round(days / 365), 'year');
}

/** Compact day label with the weekday, e.g. "ven. 24 juil." / "Fri, Jul 24". */
export function shortDayLabel(airDate: string, locale: string): string {
  return new Date(`${airDate}T00:00:00`).toLocaleDateString(locale, {
    weekday: 'short',
    day: 'numeric',
    month: 'short',
  });
}

/** Month heading label, e.g. "juillet 2026" / "July 2026". */
export function monthLabel(airDate: string, locale: string): string {
  return new Date(`${airDate}T00:00:00`).toLocaleDateString(locale, {
    month: 'long',
    year: 'numeric',
  });
}

/** Stable `YYYY-MM` month bucket key (locale-independent). */
export function monthKey(airDate: string): string {
  return airDate.slice(0, 7);
}
