// Admin "Demandes" queue: every media request as a searchable / filterable
// table (mirroring the pipeline console's shape), with quick approve/deny and
// a detail drawer. Backed by GET /api/requests + the request.updated WS event.

import { KromaEvents, type MediaRequest, type RequestStatus } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconInbox } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { RequestDrawer } from '#web/features/admin/request-drawer';
import { RequestRowView } from '#web/features/admin/request-row';
import { PageHeader, useCap, usePoll } from '#web/features/admin/shell';
import {
  Chip,
  ConsoleSearch,
  ConsoleSummary,
  ConsoleToast,
  Head,
  useConsoleToast,
  useThrottledReload,
} from '#web/features/admin/table-console';
import { apiBase } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { EmptyState, TableSkeleton } from '#web/shared/ui';

/** Filter-chip buckets over the wire statuses. */
const BUCKETS: Record<string, (s: RequestStatus) => boolean> = {
  pending: (s) => s === 'pending',
  active: (s) =>
    s === 'approved' ||
    s === 'searching' ||
    s === 'downloading' ||
    s === 'importing' ||
    s === 'partially_available',
  available: (s) => s === 'available',
  closed: (s) => s === 'denied' || s === 'failed',
  all: () => true,
};

export function RequestsQueuePage() {
  const t = useT();
  const { client } = useAuth();
  const canReview = useCap('requests.manage');

  const [bucket, setBucket] = useState('pending');
  const [q, setQ] = useState('');
  const { toast, flash } = useConsoleToast();

  const { data, reload } = usePoll(
    ['admin', 'requests', 'all'],
    () => client.listRequests(),
    30000,
  );

  // Event-driven refresh, coalesced (request.updated is low-frequency, but an
  // approval fans out into several transitions in a row).
  const throttledReload = useThrottledReload(reload);
  useEffect(() => {
    const ev = new KromaEvents(apiBase(), {
      onEvent: (e) => {
        if (e.type === 'request.updated') throttledReload();
      },
    });
    ev.connect();
    return () => ev.close();
  }, [throttledReload]);

  // Actions return the settle promise so the drawer can track its own busy state
  // while the queue keeps owning the toast + list refresh. The open drawer stays
  // fresh by subscribing to this same query (shared cache), so no push is needed.
  const act = (label: string, fn: () => Promise<unknown>) => {
    if (!canReview) return Promise.resolve();
    return fn()
      .then(() => {
        flash(label);
        reload();
      })
      .catch(() => flash(t('requests.actionFailed')));
  };
  const approve = (r: MediaRequest) =>
    act(`« ${r.title} » ${t('requests.toastApproved')}`, () => client.approveRequest(r.id));
  const deny = (r: MediaRequest, note?: string) =>
    act(`« ${r.title} » ${t('requests.toastDenied')}`, () => client.denyRequest(r.id, note));
  const removeReq = (r: MediaRequest) =>
    act(`« ${r.title} » ${t('requests.toastDeleted')}`, () => client.deleteRequest(r.id));

  const openDrawer = (r: MediaRequest) =>
    void RequestDrawer.call({
      req: r,
      canReview,
      onApprove: approve,
      onDeny: (req, note) => deny(req, note || undefined),
      onDelete: removeReq,
    });

  const all = data?.requests ?? [];
  const c = data?.counts;
  const needle = q.trim().toLowerCase();
  const rows = all.filter(
    (r) =>
      BUCKETS[bucket]?.(r.status) &&
      (!needle ||
        r.title.toLowerCase().includes(needle) ||
        (r.requestedByName ?? '').toLowerCase().includes(needle)),
  );

  return (
    <>
      <PageHeader
        title={t('admin.requestsTitle')}
        action={
          <ConsoleSearch value={q} onChange={setQ} placeholder={t('requests.searchPlaceholder')} />
        }
      />
      <ConsoleSummary
        total={c?.total ?? 0}
        totalLabel={t('requests.totalLabel')}
        accent={c?.pending ?? 0}
        accentLabel={t('requests.pendingLabel')}
      />

      <div className="mb-4 flex flex-wrap items-center gap-2.5">
        <Chip
          label={t('requests.filter.pending')}
          count={c?.pending}
          dot="rgba(244,243,240,.45)"
          on={bucket === 'pending'}
          onClick={() => setBucket('pending')}
        />
        <Chip
          label={t('requests.filter.active')}
          count={c?.active}
          dot="#F4B642"
          on={bucket === 'active'}
          onClick={() => setBucket('active')}
        />
        <Chip
          label={t('requests.filter.available')}
          count={c?.available}
          dot="#46D08D"
          on={bucket === 'available'}
          onClick={() => setBucket('available')}
        />
        <Chip
          label={t('requests.filter.closed')}
          count={(c?.denied ?? 0) + (c?.failed ?? 0)}
          dot="#E8536A"
          on={bucket === 'closed'}
          onClick={() => setBucket('closed')}
        />
        <Chip
          label={t('requests.filter.all')}
          count={c?.total}
          on={bucket === 'all'}
          onClick={() => setBucket('all')}
        />
      </div>

      <div className="overflow-hidden rounded-2xl border border-white/8 bg-[#121216] shadow-[0_10px_28px_rgba(0,0,0,.3)]">
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b border-white/6 bg-[#15151A] px-5 py-3 md:grid-cols-[minmax(0,1fr)_190px_110px_132px_76px]">
          <Head>{t('requests.colTitle')}</Head>
          <Head className="max-md:hidden">{t('requests.colRequester')}</Head>
          <Head className="max-md:hidden">{t('requests.colDate')}</Head>
          <Head className="max-md:hidden">{t('requests.colStatus')}</Head>
          <span />
        </div>

        {rows.map((r) => (
          <RequestRowView
            key={r.id}
            req={r}
            canReview={canReview}
            onOpen={() => openDrawer(r)}
            onApprove={() => approve(r)}
            onDeny={() => deny(r)}
          />
        ))}

        {data === null ? <TableSkeleton rows={8} /> : null}

        {data && rows.length === 0 ? (
          <div className="py-6">
            <EmptyState
              icon={<IconInbox size={32} stroke={1.5} />}
              title={all.length === 0 ? t('requests.empty') : t('requests.noMatch')}
            />
          </div>
        ) : null}
      </div>

      <ConsoleToast toast={toast} />
    </>
  );
}
