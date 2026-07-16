// "Manquants" (Wanted / Missing, Sonarr/Radarr-style): every request title that
// still has aired/released items not on disk, grouped by title. A "search all"
// button kicks the acquisition pass; a per-title button searches + grabs the
// best release for one title ("ask to watch"). Read of GET /api/requests/missing.

import { type CalendarEntry, hasPermission, posterColors, sizedImageUrl } from '@luma/core';
import { useT } from '@luma/ui';
import { IconInbox, IconLoader2, IconSearch } from '@tabler/icons-react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from '@tanstack/react-router';
import { useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';
import { userQueries } from '#web/shared/lib/queries';
import { EmptyState, PAGE_MAIN, PAGE_SUBTITLE, PAGE_TITLE, Skeleton } from '#web/shared/ui';

interface MissingGroup {
  requestId: string;
  tmdbId: number;
  kind: CalendarEntry['kind'];
  title: string;
  year: number | null;
  posterUrl: string | null;
  items: CalendarEntry[];
}

/** Fold the flat, title-sorted entries into one group per request. */
function groupByTitle(entries: CalendarEntry[]): MissingGroup[] {
  const byId = new Map<string, MissingGroup>();
  const order: string[] = [];
  for (const e of entries) {
    let g = byId.get(e.requestId);
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
      byId.set(e.requestId, g);
      order.push(e.requestId);
    }
    g.items.push(e);
  }
  return order.map((id) => byId.get(id) as MissingGroup);
}

export function MissingPage() {
  const t = useT();
  const navigate = useNavigate();
  const { user, client } = useAuth();
  const queryClient = useQueryClient();
  const query = userQueries.missing();
  const { data: entries, isPending } = useQuery({ ...query, refetchInterval: 30_000 });
  const canManage = !!user && hasPermission(user, 'requests.manage');
  const [searchAll, setSearchAll] = useState<'idle' | 'busy' | 'done'>('idle');

  const groups = groupByTitle(entries ?? []);
  const invalidate = () => queryClient.invalidateQueries({ queryKey: query.queryKey });

  const onSearchAll = () => {
    setSearchAll('busy');
    client
      .searchAllMissing()
      .then(() => {
        setSearchAll('done');
        // The pass runs on the sidecar; grabbed items leave the list on refetch.
        setTimeout(invalidate, 4000);
      })
      .catch(() => setSearchAll('idle'));
  };

  return (
    <main className={PAGE_MAIN}>
      <div className="flex items-start justify-between gap-4">
        <div>
          <h1 className={PAGE_TITLE}>{t('requests.missingTitle')}</h1>
          <p className={PAGE_SUBTITLE}>{t('requests.missingSubtitle')}</p>
        </div>
        {canManage && groups.length > 0 ? (
          <button
            type="button"
            disabled={searchAll !== 'idle'}
            onClick={onSearchAll}
            className="mt-1 inline-flex shrink-0 items-center gap-2 rounded-xl bg-accent px-4 py-2.5 text-[13.5px] font-bold text-accent-ink hover:bg-accent-hover disabled:opacity-60"
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

      {isPending ? (
        <div className="mt-6 flex flex-col gap-2.5">
          {Array.from({ length: 4 }, (_, i) => (
            // biome-ignore lint/suspicious/noArrayIndexKey: fixed-length placeholder rows
            <Skeleton key={i} className="h-[96px] rounded-2xl" />
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

      <div className="mt-6 flex flex-col gap-2.5">
        {groups.map((g) => (
          <MissingCard
            key={g.requestId}
            group={g}
            canManage={canManage}
            onSearched={invalidate}
            onOpen={() =>
              navigate({
                to: '/discover/$type/$tmdbId',
                params: { type: g.kind === 'show' ? 'tv' : 'movie', tmdbId: String(g.tmdbId) },
              })
            }
          />
        ))}
      </div>
    </main>
  );
}

function MissingCard({
  group,
  canManage,
  onSearched,
  onOpen,
}: Readonly<{
  group: MissingGroup;
  canManage: boolean;
  onSearched: () => void;
  onOpen: () => void;
}>) {
  const t = useT();
  const { client } = useAuth();
  const [busy, setBusy] = useState(false);
  const [c1, c2] = posterColors(String(group.tmdbId));
  const poster = sizedImageUrl(group.posterUrl, 92);

  const search = () => {
    setBusy(true);
    client
      .autoSearchRequest(group.requestId)
      .then(() => onSearched())
      .catch(() => undefined)
      .finally(() => setBusy(false));
  };

  const episodes = group.items.filter((i) => i.season != null && i.episode != null);

  return (
    <div className="flex items-center gap-4 rounded-2xl border border-border bg-surface-1 p-3.5">
      <button
        type="button"
        onClick={onOpen}
        className="flex min-w-0 flex-1 items-center gap-4 text-left"
      >
        <div
          className="h-[68px] w-[46px] flex-[0_0_46px] overflow-hidden rounded-lg"
          style={{ background: `linear-gradient(158deg, ${c1}, ${c2})` }}
        >
          {poster ? <img src={poster} alt="" className="h-full w-full object-cover" /> : null}
        </div>
        <div className="min-w-0">
          <div className="truncate text-[15px] font-bold">{group.title}</div>
          <div className="mt-0.5 text-[12.5px] font-semibold text-[#EFB661]">
            {group.kind === 'show'
              ? t('requests.missingCount', { count: String(episodes.length) })
              : t('requests.missingMovie')}
          </div>
          {episodes.length > 0 ? (
            <div className="mt-1 truncate text-[12px] font-medium text-dim">
              {episodes
                .slice(0, 6)
                .map(
                  (e) =>
                    `S${String(e.season).padStart(2, '0')}E${String(e.episode).padStart(2, '0')}`,
                )
                .join('  ')}
              {episodes.length > 6 ? ` +${episodes.length - 6}` : ''}
            </div>
          ) : null}
        </div>
      </button>
      {canManage ? (
        <button
          type="button"
          disabled={busy}
          onClick={search}
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
  );
}
