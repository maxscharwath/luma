// "Bientôt disponible": the coming-soon calendar of upcoming, not-yet-available
// releases (a movie's availability date + a show episode's air date), grouped by
// month and ascending by date. Read-only view over GET /api/requests/calendar.
// Releases landing within the week get the accent date treatment so the
// imminent stuff pops out of the list.

import { type CalendarEntry, episodeTag, posterColors, sizedImageUrl } from '@kroma/core';
import { Image, useLocale, useT } from '@kroma/ui';
import { IconCalendarClock, IconChecks } from '@tabler/icons-react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import {
  daysFromToday,
  monthKey,
  monthLabel,
  relativeAirDate,
  shortDayLabel,
} from '#web/features/requests/airdate';
import { userQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_SUBTITLE, PAGE_TITLE, Skeleton } from '#web/shared/ui';

/** Releases at most this many days out get the accent "imminent" date. */
const IMMINENT_DAYS = 7;

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
    if (g?.key !== key) {
      g = { key, label: monthLabel(e.airDate, locale), items: [] };
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

      {entries?.length === 0 ? (
        <EmptyState
          icon={<IconCalendarClock size={32} stroke={1.5} />}
          title={t('requests.calendarEmpty')}
          hint={t('requests.calendarEmptyHint')}
        />
      ) : null}

      {groups.map((g) => (
        <section key={g.key} className="mt-7">
          <h2 className="mb-2.5 flex items-baseline gap-2 text-[13px] font-bold uppercase tracking-wide text-dim">
            <span>{g.label}</span>
            <span className="text-[11.5px] font-semibold normal-case text-white/35">
              {t('requests.releaseCount', { count: g.items.length })}
            </span>
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
  // `episodeTag` is empty for a movie (no season/episode numbering).
  const epTag = episodeTag(entry) || t('requests.movieLabel');
  const airDate = entry.airDate;
  // Bounded on BOTH sides: a row whose date has slipped into the past (a tab
  // left open past local midnight keeps stale rows) is not "imminent".
  const days = airDate != null ? daysFromToday(airDate) : null;
  const imminent = days != null && days >= 0 && days <= IMMINENT_DAYS;

  return (
    <button
      type="button"
      onClick={onOpen}
      className="group flex items-center gap-4 rounded-2xl border border-border bg-surface-1 p-3 text-left transition-colors hover:border-white/20 hover:bg-white/2"
    >
      <div
        className="h-[60px] w-[40px] flex-[0_0_40px] overflow-hidden rounded-lg"
        style={{ background: `linear-gradient(158deg, ${c1}, ${c2})` }}
      >
        <Image src={poster} fit="cover" fill />
      </div>
      <div className="min-w-0 flex-1">
        <div className="truncate text-[15px] font-bold transition-colors group-hover:text-accent">
          {entry.title}
        </div>
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
        <div className={`text-[14px] font-bold ${imminent ? 'text-accent' : ''}`}>
          {airDate ? shortDayLabel(airDate, locale) : ''}
        </div>
        <div
          className={`text-[12px] font-medium first-letter:uppercase ${
            imminent ? 'text-accent/80' : 'text-dim'
          }`}
        >
          {relativeAirDate(airDate, locale)}
        </div>
      </div>
    </button>
  );
}
