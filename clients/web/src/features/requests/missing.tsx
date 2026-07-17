// "Manquants" (Wanted / Missing), modeled on Sonarr's Wanted > Missing: episode-
// level rows grouped under their series, each with its air date (relative) and a
// search action, plus row/series checkboxes driving a "search selected" toolbar
// and a "search all". A library-scan gap (no request yet) becomes a request on
// search ("ask to watch"); a requested title just re-runs its grab.

import { type CalendarEntry, hasPermission, posterColors, sizedImageUrl } from '@kroma/core';
import { useLocale, useT } from '@kroma/ui';
import { IconInbox, IconLoader2, IconSearch } from '@tabler/icons-react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useMemo, useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';
import { userQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_SUBTITLE, PAGE_TITLE, Skeleton } from '#web/shared/ui';

interface MissingGroup {
  /** The parent request, or null for a library-scan gap (never requested). */
  requestId: string | null;
  tmdbId: number;
  title: string;
  posterUrl: string | null;
  items: CalendarEntry[];
}

/** A stable key for one missing episode row (unique across the whole list). */
function epKey(e: CalendarEntry): string {
  return `${e.requestId ?? `tmdb:${e.tmdbId}`}:${e.season ?? 0}:${e.episode ?? 0}`;
}

/** Fold the flat, title-sorted entries into one group per title (keyed by the
 * request, or the tmdb id for a library-scan gap that has no request yet). */
function groupByTitle(entries: CalendarEntry[]): MissingGroup[] {
  const byKey = new Map<string, MissingGroup>();
  const order: string[] = [];
  for (const e of entries) {
    const key = e.requestId ?? `tmdb:${e.tmdbId}`;
    let g = byKey.get(key);
    if (!g) {
      g = {
        requestId: e.requestId,
        tmdbId: e.tmdbId,
        title: e.title,
        posterUrl: e.posterUrl,
        items: [],
      };
      byKey.set(key, g);
      order.push(key);
    }
    g.items.push(e);
  }
  return order.map((key) => byKey.get(key) as MissingGroup);
}

/** Locale-aware relative air date that scales the unit (days → months → years),
 * like Sonarr's "2 months ago". Empty for an undated row. */
function relativeAir(airDate: string | null, locale: string): string {
  if (!airDate) return '';
  const d = new Date(`${airDate}T00:00:00`);
  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const days = Math.round((d.getTime() - today.getTime()) / 86_400_000);
  const rtf = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  if (Math.abs(days) < 31) return rtf.format(days, 'day');
  const months = Math.round(days / 30);
  if (Math.abs(months) < 12) return rtf.format(months, 'month');
  return rtf.format(Math.round(days / 365), 'year');
}

export function MissingPage() {
  const t = useT();
  const navigate = useNavigate();
  const { user, client } = useAuth();
  const queryClient = useQueryClient();
  const query = userQueries.missing();
  const { data: entries, isPending } = useQuery({ ...query, refetchInterval: 30_000 });
  const canManage = !!user && hasPermission(user, 'requests.manage');

  const groups = useMemo(() => groupByTitle(entries ?? []), [entries]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<Set<string>>(new Set()); // group keys in flight
  const [searchAll, setSearchAll] = useState<'idle' | 'busy' | 'done'>('idle');

  const invalidate = () => queryClient.invalidateQueries({ queryKey: query.queryKey });

  // Acquire a subset of one group's episodes: a requested title re-runs its grab;
  // a library gap becomes a request for those episodes, then (if we can) grabs.
  const acquire = async (g: MissingGroup, items: CalendarEntry[]) => {
    if (g.requestId) {
      await client.autoSearchRequest(g.requestId);
      return;
    }
    const episodes = items
      .filter((i) => i.season != null && i.episode != null)
      .map((i) => ({ season: i.season as number, episode: i.episode as number }));
    const req = await client.createRequest({
      kind: 'show',
      tmdbId: g.tmdbId,
      seasons: null,
      episodes,
    });
    if (canManage) await client.autoSearchRequest(req.id);
  };

  const runGroup = (g: MissingGroup, items: CalendarEntry[]) => {
    const key = g.requestId ?? `tmdb:${g.tmdbId}`;
    setBusy((b) => new Set(b).add(key));
    acquire(g, items)
      .catch(() => undefined)
      .finally(() => {
        setBusy((b) => {
          const n = new Set(b);
          n.delete(key);
          return n;
        });
        setSelected(new Set());
        invalidate();
      });
  };

  const searchSelected = () => {
    for (const g of groups) {
      const picked = g.items.filter((i) => selected.has(epKey(i)));
      if (picked.length > 0) runGroup(g, picked);
    }
  };

  const onSearchAll = () => {
    setSearchAll('busy');
    client
      .searchAllMissing()
      .then(() => {
        setSearchAll('done');
        setTimeout(invalidate, 4000);
      })
      .catch(() => setSearchAll('idle'));
  };

  const toggleEp = (key: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });

  const selectedCount = selected.size;

  return (
    <main className={PAGE_MAIN}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className={PAGE_TITLE}>{t('requests.missingTitle')}</h1>
          <p className={PAGE_SUBTITLE}>{t('requests.missingSubtitle')}</p>
        </div>
        {groups.length > 0 ? (
          <div className="mt-1 flex items-center gap-2">
            {canManage && selectedCount > 0 ? (
              <button
                type="button"
                onClick={searchSelected}
                className="inline-flex items-center gap-2 rounded-xl border border-accent/40 bg-accent-soft px-4 py-2.5 text-[13.5px] font-bold text-accent hover:bg-accent/15"
              >
                <IconSearch size={16} stroke={2.2} />
                {t('requests.searchSelected', { count: String(selectedCount) })}
              </button>
            ) : null}
            {canManage ? (
              <button
                type="button"
                disabled={searchAll !== 'idle'}
                onClick={onSearchAll}
                className="inline-flex items-center gap-2 rounded-xl bg-accent px-4 py-2.5 text-[13.5px] font-bold text-accent-ink hover:bg-accent-hover disabled:opacity-60"
              >
                {searchAll === 'busy' ? (
                  <IconLoader2 size={16} stroke={2.2} className="animate-spin" />
                ) : (
                  <IconSearch size={16} stroke={2.2} />
                )}
                {t(searchAll === 'done' ? 'requests.searchStarted' : 'requests.searchAll')}
              </button>
            ) : null}
          </div>
        ) : null}
      </div>

      {isPending ? (
        <div className="mt-6 flex flex-col gap-2.5">
          {Array.from({ length: 4 }, (_, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder rows
            <Skeleton key={i} className="h-[120px] rounded-2xl" />
          ))}
        </div>
      ) : null}

      {entries && entries.length === 0 ? (
        <EmptyState
          icon={<IconInbox size={32} stroke={1.5} />}
          title={t('requests.missingEmpty')}
          hint={t('requests.missingEmptyHint')}
        />
      ) : null}

      <div className="mt-6 flex flex-col gap-3">
        {groups.map((g) => (
          <SeriesGroup
            key={g.requestId ?? `tmdb:${g.tmdbId}`}
            group={g}
            canManage={canManage}
            busy={busy.has(g.requestId ?? `tmdb:${g.tmdbId}`)}
            selected={selected}
            onToggleEp={toggleEp}
            onToggleSeries={(pick) =>
              setSelected((s) => {
                const n = new Set(s);
                for (const k of g.items.map(epKey)) {
                  if (pick) n.add(k);
                  else n.delete(k);
                }
                return n;
              })
            }
            onSearchSeries={() => runGroup(g, g.items)}
            onSearchEp={(item) => runGroup(g, [item])}
            onOpen={() =>
              navigate({
                to: '/discover/$type/$tmdbId',
                params: { type: 'tv', tmdbId: String(g.tmdbId) },
              })
            }
          />
        ))}
      </div>
    </main>
  );
}

function SeriesGroup({
  group,
  canManage,
  busy,
  selected,
  onToggleEp,
  onToggleSeries,
  onSearchSeries,
  onSearchEp,
  onOpen,
}: Readonly<{
  group: MissingGroup;
  canManage: boolean;
  busy: boolean;
  selected: Set<string>;
  onToggleEp: (key: string) => void;
  onToggleSeries: (pick: boolean) => void;
  onSearchSeries: () => void;
  onSearchEp: (item: CalendarEntry) => void;
  onOpen: () => void;
}>) {
  const t = useT();
  const locale = useLocale();
  const [c1, c2] = posterColors(String(group.tmdbId));
  const poster = sizedImageUrl(group.posterUrl, 92);
  const episodes = group.items.filter((i) => i.season != null && i.episode != null);
  const allPicked = episodes.length > 0 && episodes.every((e) => selected.has(epKey(e)));
  // A gap is actionable by any requester; a request needs manage.
  const canAct = group.requestId ? canManage : true;

  return (
    <section className="overflow-hidden rounded-2xl border border-border bg-surface-1">
      <div className="flex items-center gap-3.5 border-b border-white/[0.06] p-3.5">
        {canAct ? (
          <Check on={allPicked} onClick={() => onToggleSeries(!allPicked)} />
        ) : (
          <span className="w-[18px]" />
        )}
        <button
          type="button"
          onClick={onOpen}
          className="flex min-w-0 flex-1 items-center gap-3.5 text-left"
        >
          <div
            className="h-[52px] w-[36px] flex-[0_0_36px] overflow-hidden rounded-md"
            style={{ background: `linear-gradient(158deg, ${c1}, ${c2})` }}
          >
            {poster ? <img src={poster} alt="" className="h-full w-full object-cover" /> : null}
          </div>
          <div className="min-w-0">
            <div className="truncate text-[15px] font-bold">{group.title}</div>
            <div className="mt-0.5 text-[12.5px] font-semibold text-[#EFB661]">
              {t('requests.missingCount', { count: String(episodes.length) })}
            </div>
          </div>
        </button>
        {canAct ? (
          <button
            type="button"
            disabled={busy}
            onClick={onSearchSeries}
            title={t('requests.searchTitle')}
            className="inline-flex h-9 items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3 text-[12.5px] font-bold text-white/75 hover:text-accent disabled:opacity-50"
          >
            {busy ? (
              <IconLoader2 size={15} stroke={2.4} className="animate-spin" />
            ) : (
              <IconSearch size={15} stroke={2.2} />
            )}
            {t('requests.search')}
          </button>
        ) : null}
      </div>
      <ul className="divide-y divide-white/[0.04]">
        {episodes.map((e) => {
          const key = epKey(e);
          const rel = relativeAir(e.airDate, locale);
          return (
            <li key={key} className="flex items-center gap-3.5 px-3.5 py-2.5">
              {canAct ? (
                <Check on={selected.has(key)} onClick={() => onToggleEp(key)} />
              ) : (
                <span className="w-[18px]" />
              )}
              <span className="w-[62px] flex-[0_0_62px] font-mono text-[13px] font-bold text-accent tabular-nums">
                S{String(e.season).padStart(2, '0')}E{String(e.episode).padStart(2, '0')}
              </span>
              <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-dim">
                {rel ? <span className="first-letter:uppercase">{rel}</span> : null}
              </span>
              {canAct ? (
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onSearchEp(e)}
                  title={t('requests.searchTitle')}
                  className="flex h-8 w-8 items-center justify-center rounded-lg text-white/45 hover:bg-white/5 hover:text-accent disabled:opacity-40"
                >
                  <IconSearch size={15} stroke={2} />
                </button>
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function Check({ on, onClick }: Readonly<{ on: boolean; onClick: () => void }>) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      className={`flex h-[18px] w-[18px] flex-[0_0_18px] items-center justify-center rounded-[5px] border transition-colors ${
        on ? 'border-accent bg-accent text-accent-ink' : 'border-white/25 hover:border-white/50'
      }`}
    >
      {on ? (
        <svg viewBox="0 0 12 12" width="11" height="11" fill="none" aria-hidden="true">
          <path
            d="M2 6.2 4.6 9 10 3"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      ) : null}
    </button>
  );
}
