// "Bientôt disponible": the coming-soon calendar of upcoming, not-yet-available
// releases (a movie's availability date + a show episode's air date), grouped by
// month and ascending by date. Read-only view over GET /api/requests/calendar.

import { type CalendarEntry, posterColors, sizedImageUrl } from '@kroma/core';
import { useLocale, useT } from '@kroma/ui';
import { IconCalendarClock, IconChecks } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { userQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_SUBTITLE, PAGE_TITLE, Skeleton } from '#web/shared/ui';

/** `YYYY-MM` bucket key for month grouping (stable, locale-independent). */
function monthKey(airDate: string): string {
  return airDate.slice(0, 7);
}

export function ComingSoonPage() {
  const t = useT();
  const locale = useLocale();
  const navigate = useNavigate();
  const { data: entries, isPending } = useQuery({
    ...userQueries.calendar(),
    refetchInterval: 60_000,
  });

  // Group the (already date-sorted) entries by month, preserving order. Calendar
  // entries are always dated (the server filters to future dates); the guard is
  // for the shared, nullable-airDate type.
  const groups: Array<{ key: string; label: string; items: CalendarEntry[] }> = [];
  for (const e of entries ?? []) {
    if (!e.airDate) continue;
    const key = monthKey(e.airDate);
    let g = groups.at(-1);
    if (!g || g.key !== key) {
      const label = new Date(`${e.airDate}T00:00:00`).toLocaleDateString(locale, {
        month: 'long',
        year: 'numeric',
      });
      g = { key, label, items: [] };
      groups.push(g);
    }
    g.items.push(e);
  }

  return (
    <main className={PAGE_MAIN}>
      <h1 className={PAGE_TITLE}>{t('requests.calendarTitle')}</h1>
      <p className={PAGE_SUBTITLE}>{t('requests.calendarSubtitle')}</p>

      {isPending ? (
        <div className="mt-6 flex flex-col gap-2.5">
          {Array.from({ length: 5 }, (_, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder rows
            <Skeleton key={i} className="h-[76px] rounded-2xl" />
          ))}
        </div>
      ) : null}

      {entries && entries.length === 0 ? (
        <EmptyState
          icon={<IconCalendarClock size={32} stroke={1.5} />}
          title={t('requests.calendarEmpty')}
          hint={t('requests.calendarEmptyHint')}
        />
      ) : null}

      {groups.map((g) => (
        <section key={g.key} className="mt-7">
          <h2 className="mb-2.5 text-[13px] font-bold uppercase tracking-wide text-dim first-letter:uppercase">
            {g.label}
          </h2>
          <div className="flex flex-col gap-2.5">
            {g.items.map((e) => (
              <CalendarRow
                key={`${e.requestId}:${e.season ?? 0}:${e.episode ?? 0}`}
                entry={e}
                locale={locale}
                onOpen={() =>
                  navigate({
                    to: '/discover/$type/$tmdbId',
                    params: {
                      type: e.kind === 'show' ? 'tv' : 'movie',
                      tmdbId: String(e.tmdbId),
                    },
                  })
                }
              />
            ))}
          </div>
        </section>
      ))}
    </main>
  );
}

function CalendarRow({
  entry,
  locale,
  onOpen,
}: Readonly<{ entry: CalendarEntry; locale: string; onOpen: () => void }>) {
  const t = useT();
  const [c1, c2] = posterColors(String(entry.tmdbId));
  const poster = sizedImageUrl(entry.posterUrl, 92);
  const isEpisode = entry.season != null && entry.episode != null;
  const epTag = isEpisode
    ? `S${String(entry.season).padStart(2, '0')}E${String(entry.episode).padStart(2, '0')}`
    : t('requests.movieLabel');
  const date = entry.airDate ? new Date(`${entry.airDate}T00:00:00`) : null;
  const dateLabel = date ? date.toLocaleDateString(locale, { day: 'numeric', month: 'short' }) : '';
  const relative = date ? relativeDays(date, locale) : '';

  return (
    <button
      type="button"
      onClick={onOpen}
      className="flex items-center gap-4 rounded-2xl border border-border bg-surface-1 p-3 text-left hover:border-white/20"
    >
      <div
        className="h-[60px] w-[40px] flex-[0_0_40px] overflow-hidden rounded-lg"
        style={{ background: `linear-gradient(158deg, ${c1}, ${c2})` }}
      >
        {poster ? <img src={poster} alt="" className="h-full w-full object-cover" /> : null}
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[15px] font-bold">{entry.title}</div>
        <div className="mt-0.5 flex items-center gap-1.5 text-[12.5px] font-semibold text-dim">
          <span className="text-accent">{epTag}</span>
          {entry.year ? <span>· {entry.year}</span> : null}
          {entry.status === 'grabbed' ? (
            <span className="inline-flex items-center gap-0.5 text-[#5FD08A]">
              · <IconChecks size={13} stroke={2} /> {t('requests.securedShort')}
            </span>
          ) : null}
        </div>
      </div>
      <div className="flex flex-col items-end text-right">
        <div className="text-[14px] font-bold">{dateLabel}</div>
        <div className="text-[12px] font-medium text-dim first-letter:uppercase">{relative}</div>
      </div>
    </button>
  );
}

/** Locale-aware "in N days" / "tomorrow" for a future date, no i18n keys. */
function relativeDays(date: Date, locale: string): string {
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const days = Math.round((date.getTime() - today.getTime()) / 86_400_000);
  return new Intl.RelativeTimeFormat(locale, { numeric: 'auto' }).format(days, 'day');
}
