// Admin "Demandes" queue: every media request as a searchable / filterable
// table (mirroring the pipeline console's shape), with quick approve/deny and
// a detail drawer. Backed by GET /api/requests + the request.updated WS event.

import { LumaEvents, type MediaRequest, type RequestStatus } from '@luma/core';
import { useT } from '@luma/ui';
import { IconSearch, IconX } from '@tabler/icons-react';
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { RequestDrawer } from '#web/features/admin/requestDrawer';
import { RequestRowView } from '#web/features/admin/requestRow';
import { useAdmin, useCap, usePoll } from '#web/features/admin/shell';
import { apiBase } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { InputGroup, InputGroupAddon, InputGroupInput } from '#web/shared/ui/InputGroup';

/** Filter-chip buckets over the wire statuses. */
const BUCKETS: Record<string, (s: RequestStatus) => boolean> = {
  pending: (s) => s === 'pending',
  active: (s) =>
    s === 'approved' || s === 'searching' || s === 'downloading' || s === 'importing' || s === 'partially_available',
  available: (s) => s === 'available',
  closed: (s) => s === 'denied' || s === 'failed',
  all: () => true,
};

export function RequestsQueuePage() {
  const t = useT();
  const { client } = useAuth();
  const { tick } = useAdmin();
  const canReview = useCap('requests.manage');

  const [bucket, setBucket] = useState('pending');
  const [q, setQ] = useState('');
  const [drawer, setDrawer] = useState<MediaRequest | null>(null);
  const [busy, setBusy] = useState(false);
  const [toast, setToast] = useState<{ text: string; on: boolean }>({ text: '', on: false });

  const { data, reload } = usePoll(() => client.listRequests(), 30000, [client, tick]);

  // Event-driven refresh, coalesced (request.updated is low-frequency, but an
  // approval fans out into several transitions in a row).
  const lastReloadRef = useRef(0);
  const throttledReload = useCallback(() => {
    const now = Date.now();
    if (now - lastReloadRef.current < 1500) return;
    lastReloadRef.current = now;
    reload();
  }, [reload]);
  useEffect(() => {
    const ev = new LumaEvents(apiBase(), {
      onEvent: (e) => {
        if (e.type === 'request.updated') throttledReload();
      },
    });
    ev.connect();
    return () => ev.close();
  }, [throttledReload]);

  // Keep the open drawer fresh across reloads.
  useEffect(() => {
    if (!drawer || !data) return;
    const fresh = data.requests.find((r) => r.id === drawer.id);
    setDrawer(fresh ?? null);
  }, [data, drawer]);

  const flash = (text: string) => {
    setToast({ text, on: true });
    window.setTimeout(() => setToast((s) => ({ ...s, on: false })), 2800);
  };
  const act = (label: string, fn: () => Promise<unknown>) => {
    if (!canReview) return;
    setBusy(true);
    fn()
      .then(() => {
        flash(label);
        reload();
      })
      .catch(() => flash(t('requests.actionFailed')))
      .finally(() => setBusy(false));
  };
  const approve = (r: MediaRequest) =>
    act(`« ${r.title} » ${t('requests.toastApproved')}`, () => client.approveRequest(r.id));
  const deny = (r: MediaRequest, note?: string) =>
    act(`« ${r.title} » ${t('requests.toastDenied')}`, () => client.denyRequest(r.id, note));
  const removeReq = (r: MediaRequest) => {
    setDrawer(null);
    act(`« ${r.title} » ${t('requests.toastDeleted')}`, () => client.deleteRequest(r.id));
  };

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
    <main className="min-w-0 max-w-[1280px] px-11 pb-20 pt-[30px]">
      <div className="mb-5 flex items-start justify-between gap-6">
        <div className="min-w-0">
          <h1 className="font-display text-[34px] font-bold leading-[1.05] tracking-[-.02em]">
            {t('admin.requestsTitle')}
          </h1>
          <p className="mt-2 text-[14.5px] font-medium text-white/50">
            <span className="font-bold text-white">{(c?.total ?? 0).toLocaleString()}</span>{' '}
            {t('requests.totalLabel')} ·{' '}
            <span className="font-bold text-accent">{(c?.pending ?? 0).toLocaleString()}</span>{' '}
            {t('requests.pendingLabel')}
          </p>
        </div>
        <div className="w-80">
          <InputGroup className="h-11">
            <InputGroupAddon>
              <IconSearch size={17} />
            </InputGroupAddon>
            <InputGroupInput
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder={t('requests.searchPlaceholder')}
              className="text-[14px] font-semibold"
            />
            {q ? (
              <button type="button" onClick={() => setQ('')} className="shrink-0 text-white/50 hover:text-white">
                <IconX size={16} stroke={2.2} />
              </button>
            ) : null}
          </InputGroup>
        </div>
      </div>

      <div className="mb-4 flex flex-wrap items-center gap-2.5">
        <Chip label={t('requests.filter.pending')} count={c?.pending} dot="rgba(244,243,240,.45)" on={bucket === 'pending'} onClick={() => setBucket('pending')} />
        <Chip label={t('requests.filter.active')} count={c?.active} dot="#F4B642" on={bucket === 'active'} onClick={() => setBucket('active')} />
        <Chip label={t('requests.filter.available')} count={c?.available} dot="#46D08D" on={bucket === 'available'} onClick={() => setBucket('available')} />
        <Chip label={t('requests.filter.closed')} count={(c?.denied ?? 0) + (c?.failed ?? 0)} dot="#E8536A" on={bucket === 'closed'} onClick={() => setBucket('closed')} />
        <Chip label={t('requests.filter.all')} count={c?.total} on={bucket === 'all'} onClick={() => setBucket('all')} />
      </div>

      <div className="overflow-hidden rounded-2xl border border-white/[0.08] bg-[#121216] shadow-[0_10px_28px_rgba(0,0,0,.3)]">
        <div className="grid grid-cols-[minmax(0,1fr)_190px_110px_132px_76px] gap-4 border-b border-white/[0.06] bg-[#15151A] px-5 py-3">
          <Head>{t('requests.colTitle')}</Head>
          <Head>{t('requests.colRequester')}</Head>
          <Head>{t('requests.colDate')}</Head>
          <Head>{t('requests.colStatus')}</Head>
          <span />
        </div>

        {rows.map((r) => (
          <RequestRowView
            key={r.id}
            req={r}
            canReview={canReview}
            onOpen={() => setDrawer(r)}
            onApprove={() => approve(r)}
            onDeny={() => deny(r)}
          />
        ))}

        {data && rows.length === 0 ? (
          <div className="px-5 py-14 text-center text-[14px] font-medium text-white/45">
            {all.length === 0 ? t('requests.empty') : t('requests.noMatch')}
          </div>
        ) : null}
      </div>

      <RequestDrawer
        req={drawer}
        busy={busy}
        canReview={canReview}
        onClose={() => setDrawer(null)}
        onApprove={() => drawer && approve(drawer)}
        onDeny={(note) => drawer && deny(drawer, note || undefined)}
        onDelete={() => drawer && removeReq(drawer)}
      />

      <div
        className="pointer-events-none fixed bottom-6 left-1/2 z-[80] -translate-x-1/2 transition-all duration-200"
        style={{ opacity: toast.on ? 1 : 0, transform: `translateX(-50%) translateY(${toast.on ? 0 : 12}px)` }}
      >
        <div className="inline-flex items-center gap-2.5 rounded-full border border-white/12 bg-[#1C1C22] px-[18px] py-2.5 shadow-[0_20px_50px_rgba(0,0,0,.55)]">
          <span className="h-2 w-2 flex-[0_0_8px] rounded-full bg-accent" />
          <span className="text-[13.5px] font-semibold text-white">{toast.text}</span>
        </div>
      </div>
    </main>
  );
}

function Head({ children }: Readonly<{ children: ReactNode }>) {
  return <span className="text-[9.5px] font-bold uppercase tracking-[.12em] text-white/40">{children}</span>;
}

function Chip({
  label,
  count,
  dot,
  on,
  onClick,
}: Readonly<{ label: string; count?: number; dot?: string; on: boolean; onClick: () => void }>) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`inline-flex items-center gap-2 rounded-full border px-3.5 py-2 text-[13px] font-semibold transition-colors ${on ? 'border-accent/35 bg-accent/[0.14] text-accent' : 'border-white/[0.08] bg-[#15151A] text-white/65'}`}
    >
      {dot ? <span className="h-[7px] w-[7px] rounded-full" style={{ background: dot }} /> : null}
      {label}
      {count != null ? <span className="tabular-nums opacity-60">{count.toLocaleString()}</span> : null}
    </button>
  );
}
