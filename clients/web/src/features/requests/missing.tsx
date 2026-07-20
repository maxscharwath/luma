// "Manquants" (Wanted / Missing), modeled on Sonarr's Wanted > Missing: episode-
// level rows grouped under their series (or a single movie row), each with its
// air date (relative) and a search action, plus row/series checkboxes driving a
// "search selected" toolbar and a "search all". A library-scan gap (no request
// yet) becomes a request on search ("ask to watch"); a requested title just
// re-runs its grab. The group card itself lives in `missing-group.tsx`.

import { type CalendarEntry, hasPermission } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconInbox, IconLoader2, IconSearch } from '@tabler/icons-react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useMemo, useState } from 'react';
import { epKey, type MissingGroup, MissingGroupCard } from '#web/features/requests/missing-group';
import { useAuth } from '#web/shared/lib/auth';
import { userQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_SUBTITLE, PAGE_TITLE, Skeleton } from '#web/shared/ui';

/** Toggle a single row key in a selection set (returns a fresh set). */
function toggleKey(prev: Set<string>, key: string): Set<string> {
  const n = new Set(prev);
  if (n.has(key)) n.delete(key);
  else n.add(key);
  return n;
}

/** Add or remove a batch of row keys in a set (returns a fresh set). */
function toggleKeys(prev: Set<string>, keys: string[], pick: boolean): Set<string> {
  const n = new Set(prev);
  for (const k of keys) {
    if (pick) n.add(k);
    else n.delete(k);
  }
  return n;
}

/** State of the "search all" button: idle, in flight, or fired (the grabs run
 * server-side, so "done" only means the batch was started). */
type SearchAllState = 'idle' | 'busy' | 'done';

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
        kind: e.kind,
        title: e.title,
        year: e.year,
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
  const [busyKeys, setBusyKeys] = useState<Set<string>>(new Set()); // row keys in flight
  const [searchAll, setSearchAll] = useState<SearchAllState>('idle');

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
    const keys = items.map(epKey);
    setBusyKeys((b) => toggleKeys(b, keys, true));
    acquire(g, items)
      .catch(() => undefined)
      .finally(() => {
        setBusyKeys((b) => toggleKeys(b, keys, false));
        // Only the rows that were searched leave the selection; picks on other
        // titles survive a per-row / per-series search.
        setSelected((s) => toggleKeys(s, keys, false));
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

  return (
    <main className={PAGE_MAIN}>
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h1 className={PAGE_TITLE}>{t('requests.missingTitle')}</h1>
          <p className={PAGE_SUBTITLE}>{t('requests.missingSubtitle')}</p>
        </div>
        {groups.length > 0 ? (
          <MissingActions
            canManage={canManage}
            selectedCount={selected.size}
            searchAll={searchAll}
            onSearchSelected={searchSelected}
            onSearchAll={onSearchAll}
          />
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

      {entries?.length === 0 ? (
        <EmptyState
          icon={<IconInbox size={32} stroke={1.5} />}
          title={t('requests.missingEmpty')}
          hint={t('requests.missingEmptyHint')}
        />
      ) : null}

      {entries && entries.length > 0 ? (
        <p className="mt-5 text-[13px] font-semibold text-dim">
          {t('requests.missingCount', { count: entries.length })}
          {' · '}
          {t('person.titleCount', { count: groups.length })}
        </p>
      ) : null}

      <div className="mt-3 flex flex-col gap-3">
        {groups.map((g) => (
          <MissingGroupCard
            key={g.requestId ?? `tmdb:${g.tmdbId}`}
            group={g}
            canManage={canManage}
            busyKeys={busyKeys}
            selected={selected}
            onToggleRow={(key) => setSelected((s) => toggleKey(s, key))}
            onToggleGroup={(pick) => setSelected((s) => toggleKeys(s, g.items.map(epKey), pick))}
            onSearch={(items) => runGroup(g, items)}
            onOpen={() =>
              navigate({
                to: '/discover/$type/$tmdbId',
                params: {
                  type: g.kind === 'movie' ? 'movie' : 'tv',
                  tmdbId: String(g.tmdbId),
                },
              })
            }
          />
        ))}
      </div>
    </main>
  );
}

/** The page's toolbar: "search selected" (only while rows are picked) and
 * "search all". Both are manage-only; a requester just sees the list. */
function MissingActions({
  canManage,
  selectedCount,
  searchAll,
  onSearchSelected,
  onSearchAll,
}: Readonly<{
  canManage: boolean;
  selectedCount: number;
  searchAll: SearchAllState;
  onSearchSelected: () => void;
  onSearchAll: () => void;
}>) {
  const t = useT();
  return (
    <div className="mt-1 flex items-center gap-2">
      {canManage && selectedCount > 0 ? (
        <button
          type="button"
          onClick={onSearchSelected}
          className="inline-flex items-center gap-2 rounded-xl border border-accent/40 bg-accent-soft px-4 py-2.5 text-[13.5px] font-bold text-accent hover:bg-accent/15"
        >
          <IconSearch size={16} stroke={2.2} />
          {t('requests.searchSelected', { count: selectedCount })}
        </button>
      ) : null}
      {canManage ? <SearchAllButton state={searchAll} onClick={onSearchAll} /> : null}
    </div>
  );
}

/** "Search all missing": fires one server-side batch, then reads as started
 * (the grabs land asynchronously, the list refreshes a few seconds later). */
function SearchAllButton({
  state,
  onClick,
}: Readonly<{ state: SearchAllState; onClick: () => void }>) {
  const t = useT();
  return (
    <button
      type="button"
      disabled={state !== 'idle'}
      onClick={onClick}
      className="inline-flex items-center gap-2 rounded-xl bg-accent px-4 py-2.5 text-[13.5px] font-bold text-accent-ink hover:bg-accent-hover disabled:opacity-60"
    >
      {state === 'busy' ? (
        <IconLoader2 size={16} stroke={2.2} className="animate-spin" />
      ) : (
        <IconSearch size={16} stroke={2.2} />
      )}
      {t(state === 'done' ? 'requests.searchStarted' : 'requests.searchAll')}
    </button>
  );
}
