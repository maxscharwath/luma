// Admin "Pipeline de traitement" console, element-centric: the whole catalog as
// a searchable / filterable table where each film, series and episode shows the
// status of every treatment applied to it, with a detail drawer to inspect and
// reprocess. Backed by GET /api/admin/pipeline/elements + the pipeline.stats WS.

import { type ElementRow, LumaEvents, type MessageKey } from '@luma/core';
import { useT } from '@luma/ui';
import {
  IconChevronLeft,
  IconChevronRight,
  IconPlayerPause,
  IconPlayerPlay,
  IconSearch,
  IconX,
} from '@tabler/icons-react';
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { PipelineDrawer } from '#web/features/admin/pipeline-drawer';
import { ElementRowView } from '#web/features/admin/pipeline-row';
import { useCap, usePoll } from '#web/features/admin/shell';
import { apiBase } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { InputGroup, InputGroupAddon, InputGroupInput } from '#web/shared/ui/input-group';

const PER_PAGE = 30;
const apiKind = (el: ElementRow): 'item' | 'show' => (el.kind === 'series' ? 'show' : 'item');

export function PipelinePage() {
  const t = useT();
  const { client } = useAuth();
  const canManage = useCap('settings.manage');

  const [status, setStatus] = useState('attention');
  const [kind, setKind] = useState('all');
  const [q, setQ] = useState('');
  const [dq, setDq] = useState('');
  const [page, setPage] = useState(0);
  const [drawer, setDrawer] = useState<ElementRow | null>(null);
  const [busy, setBusy] = useState(false);
  const [paused, setPaused] = useState(false);
  const [toast, setToast] = useState<{ text: string; on: boolean }>({ text: '', on: false });

  // Debounce the search box.
  useEffect(() => {
    const h = setTimeout(() => {
      setDq(q);
      setPage(0);
    }, 250);
    return () => clearTimeout(h);
  }, [q]);

  // Refresh is event-driven (WS push), with a slow poll only as a reconnect/
  // missed-event safety net, not a tight interval hammering the endpoint.
  const { data, reload } = usePoll(
    ['admin', 'pipeline', 'elements', status, kind, dq, page],
    () => client.pipelineElements({ status, kind, q: dq, page, limit: PER_PAGE }),
    30000,
  );

  // Throttle event-driven reloads: a draining stage fires pipeline.stats ~1/s and
  // enrich fires many item.updated; coalesce to at most one refetch per 1.5 s.
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
        if (
          e.type === 'pipeline.stats' ||
          e.type === 'job.finished' ||
          e.type === 'job.started' ||
          e.type === 'item.updated' ||
          e.type === 'show.updated' ||
          e.type === 'library.updated'
        ) {
          throttledReload();
        }
      },
    });
    ev.connect();
    return () => ev.close();
  }, [throttledReload]);

  // Keep the open drawer fresh from reloads while its element is on the page.
  useEffect(() => {
    if (!drawer || !data) return;
    const fresh = data.elements.find((e) => e.id === drawer.id);
    if (fresh) setDrawer(fresh);
  }, [data, drawer]);

  // The global pause flag lives on the pipeline-health endpoint; poll it (on the
  // admin shell's tick) so another admin's toggle shows, and mirror it into local
  // state so the toggle can update optimistically.
  const { data: health } = usePoll(
    ['admin', 'pipeline', 'health'],
    () => client.adminPipeline(),
    30000,
  );
  useEffect(() => {
    if (health) setPaused(health.paused);
  }, [health]);

  const flash = (text: string) => {
    setToast({ text, on: true });
    window.setTimeout(() => setToast((s) => ({ ...s, on: false })), 2800);
  };
  const togglePause = () => {
    if (!canManage) return;
    const next = !paused;
    setPaused(next); // optimistic
    client
      .pausePipeline(next)
      .then((r) => {
        setPaused(r.paused);
        flash(t(next ? 'pipeline.toastPaused' : 'pipeline.toastResumed'));
      })
      .catch(() => setPaused(!next));
  };
  const reprocess = (el: ElementRow) => {
    if (!canManage) return;
    setBusy(true);
    client
      .reprocessSubject(apiKind(el), el.id)
      .then(() => {
        flash(`« ${el.title} » ${t('pipeline.toastReprocess')}`);
        reload();
      })
      .finally(() => setBusy(false));
  };
  const retryStage = (el: ElementRow, stage: string) => {
    if (!canManage) return;
    setBusy(true);
    client
      .retryElementStage(apiKind(el), el.id, stage)
      .then(() => {
        flash(`${t(`pipeline.t.${stage}` as MessageKey)} ${t('pipeline.toastRetry')}`);
        reload();
      })
      .finally(() => setBusy(false));
  };

  const c = data?.counts;
  const total = c?.total ?? 0;
  const attention = c ? c.failed + c.running + c.pending : 0;
  const rows = data?.elements ?? [];
  const start = page * PER_PAGE;

  const pick = (setter: (v: string) => void) => (v: string) => {
    setter(v);
    setPage(0);
  };

  return (
    <main className="min-w-0 max-w-[1280px] px-11 pb-20 pt-[30px]">
      {/* header */}
      <div className="mb-5 flex items-start justify-between gap-6">
        <div className="min-w-0">
          <h1 className="font-display text-[34px] font-bold leading-[1.05] tracking-[-.02em]">
            {t('admin.pipelineTitle')}
          </h1>
          <p className="mt-2 text-[14.5px] font-medium text-white/50">
            <span className="font-bold text-white">{total.toLocaleString()}</span>{' '}
            {t('pipeline.trackedLabel')} ·{' '}
            <span className="font-bold text-accent">{attention.toLocaleString()}</span>{' '}
            {t('pipeline.needActionLabel')}
          </p>
        </div>
        <div className="flex flex-[0_0_auto] items-center gap-3">
          {canManage ? (
            <button
              type="button"
              onClick={togglePause}
              title={t(paused ? 'pipeline.resumeHint' : 'pipeline.pauseHint')}
              className={`inline-flex h-11 items-center gap-2 rounded-xl border px-4 text-[13.5px] font-semibold transition-colors ${
                paused
                  ? 'border-[#46D08D]/40 bg-[#46D08D]/[0.14] text-[#46D08D] hover:bg-[#46D08D]/20'
                  : 'border-white/12 bg-[#1A1A20] text-white/80 hover:bg-[#222229]'
              }`}
            >
              {paused ? (
                <IconPlayerPlay size={15} stroke={2} />
              ) : (
                <IconPlayerPause size={15} stroke={2} />
              )}
              {t(paused ? 'pipeline.resume' : 'pipeline.pause')}
            </button>
          ) : null}
          <div className="w-80">
            <InputGroup className="h-11">
              <InputGroupAddon>
                <IconSearch size={17} />
              </InputGroupAddon>
              <InputGroupInput
                value={q}
                onChange={(e) => setQ(e.target.value)}
                placeholder={t('pipeline.searchPlaceholder')}
                className="text-[14px] font-semibold"
              />
              {q ? (
                <button
                  type="button"
                  onClick={() => setQ('')}
                  className="shrink-0 text-white/50 hover:text-white"
                >
                  <IconX size={16} stroke={2.2} />
                </button>
              ) : null}
            </InputGroup>
          </div>
        </div>
      </div>

      {/* paused banner */}
      {paused ? (
        <div className="mb-4 flex items-center gap-2.5 rounded-xl border border-[#F4B642]/30 bg-[#F4B642]/[0.10] px-4 py-2.5 text-[13.5px] font-semibold text-[#F4B642]">
          <IconPlayerPause size={15} stroke={2} />
          {t('pipeline.pausedBanner')}
        </div>
      ) : null}

      {/* filters */}
      <div className="mb-4 flex flex-wrap items-center gap-2.5">
        <Chip
          label={t('pipeline.filter.attention')}
          count={attention}
          dot="#F4B642"
          on={status === 'attention'}
          tone="accent"
          onClick={() => pick(setStatus)('attention')}
        />
        <Chip
          label={t('pipeline.filter.failed')}
          count={c?.failed}
          dot="#E8536A"
          on={status === 'failed'}
          tone="accent"
          onClick={() => pick(setStatus)('failed')}
        />
        <Chip
          label={t('pipeline.filter.running')}
          count={c?.running}
          dot="#F4B642"
          on={status === 'running'}
          tone="accent"
          onClick={() => pick(setStatus)('running')}
        />
        <Chip
          label={t('pipeline.filter.pending')}
          count={c?.pending}
          dot="rgba(244,243,240,.45)"
          on={status === 'pending'}
          tone="accent"
          onClick={() => pick(setStatus)('pending')}
        />
        <Chip
          label={t('pipeline.filter.ok')}
          count={c?.ok}
          dot="#46D08D"
          on={status === 'ok'}
          tone="accent"
          onClick={() => pick(setStatus)('ok')}
        />
        <Chip
          label={t('pipeline.filter.all')}
          count={total}
          on={status === 'all'}
          tone="accent"
          onClick={() => pick(setStatus)('all')}
        />
        <span className="mx-1 h-[22px] w-px bg-white/12" />
        <Chip
          label={t('pipeline.filter.allTypes')}
          count={total}
          on={kind === 'all'}
          tone="blue"
          onClick={() => pick(setKind)('all')}
        />
        <Chip
          label={t('pipeline.filter.films')}
          count={c?.film}
          on={kind === 'film'}
          tone="blue"
          onClick={() => pick(setKind)('film')}
        />
        <Chip
          label={t('pipeline.filter.series')}
          count={c?.series}
          on={kind === 'series'}
          tone="blue"
          onClick={() => pick(setKind)('series')}
        />
        <Chip
          label={t('pipeline.filter.episodes')}
          count={c?.episode}
          on={kind === 'episode'}
          tone="blue"
          onClick={() => pick(setKind)('episode')}
        />
      </div>

      {/* table */}
      <div className="overflow-hidden rounded-2xl border border-white/[0.08] bg-[#121216] shadow-[0_10px_28px_rgba(0,0,0,.3)]">
        <div className="grid grid-cols-[minmax(0,1fr)_150px_132px_46px] gap-4 border-b border-white/[0.06] bg-[#15151A] px-5 py-3">
          <Head>{t('pipeline.colElement')}</Head>
          <Head>{t('pipeline.treatments')}</Head>
          <Head>{t('pipeline.colStatus')}</Head>
          <span />
        </div>

        {rows.map((el) => (
          <ElementRowView
            key={`${el.kind}-${el.id}`}
            el={el}
            onOpen={() => setDrawer(el)}
            onReprocess={() => reprocess(el)}
          />
        ))}

        {data && rows.length === 0 ? (
          <div className="px-5 py-14 text-center text-[14px] font-medium text-white/45">
            {t('pipeline.noMatch')}
          </div>
        ) : null}

        {rows.length > 0 ? (
          <div className="flex items-center justify-between gap-4 border-t border-white/[0.06] bg-[#0F0F13] px-5 py-3.5">
            <div className="flex items-center gap-4">
              <span className="text-[12.5px] font-semibold tabular-nums text-white/60">
                {(start + 1).toLocaleString()}–
                {Math.min(start + PER_PAGE, data?.total ?? 0).toLocaleString()} /{' '}
                {(data?.total ?? 0).toLocaleString()}
              </span>
              <div className="hidden items-center gap-3 md:flex">
                <Legend color="#46D08D" label={t('pipeline.st.done')} />
                <Legend color="#F4B642" label={t('pipeline.st.running')} />
                <Legend color="rgba(255,255,255,.3)" label={t('pipeline.st.pending')} />
                <Legend color="#E8536A" label={t('pipeline.st.failed')} />
              </div>
            </div>
            <div className="flex items-center gap-2.5">
              <Pager
                dir="prev"
                disabled={page <= 0}
                onClick={() => setPage((p) => Math.max(0, p - 1))}
                label={t('pipeline.prev')}
              />
              <span className="text-[12.5px] font-semibold tabular-nums text-white/55">
                {t('pipeline.page')} {page + 1} / {(data?.pages ?? 1).toLocaleString()}
              </span>
              <Pager
                dir="next"
                disabled={page >= (data?.pages ?? 1) - 1}
                onClick={() => setPage((p) => p + 1)}
                label={t('pipeline.next')}
              />
            </div>
          </div>
        ) : null}
      </div>

      <PipelineDrawer
        el={drawer}
        busy={busy}
        onClose={() => setDrawer(null)}
        onReprocess={() => drawer && reprocess(drawer)}
        onRetryStage={(stage) => drawer && retryStage(drawer, stage)}
      />

      {/* toast */}
      <div
        className="pointer-events-none fixed bottom-6 left-1/2 z-[80] -translate-x-1/2 transition-all duration-200"
        style={{
          opacity: toast.on ? 1 : 0,
          transform: `translateX(-50%) translateY(${toast.on ? 0 : 12}px)`,
        }}
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
  return (
    <span className="text-[9.5px] font-bold uppercase tracking-[.12em] text-white/40">
      {children}
    </span>
  );
}

function Legend({ color, label }: Readonly<{ color: string; label: string }>) {
  return (
    <span className="inline-flex items-center gap-1.5 text-[11px] font-semibold text-white/45">
      <span className="h-2 w-2 rounded-full" style={{ background: color }} />
      {label}
    </span>
  );
}

function Chip({
  label,
  count,
  dot,
  on,
  tone,
  onClick,
}: Readonly<{
  label: string;
  count?: number;
  dot?: string;
  on: boolean;
  tone: 'accent' | 'blue';
  onClick: () => void;
}>) {
  const active =
    tone === 'accent'
      ? 'border-accent/35 bg-accent/[0.14] text-accent'
      : 'border-[#86A8FF]/35 bg-[#86A8FF]/[0.14] text-[#86A8FF]';
  return (
    <button
      type="button"
      onClick={onClick}
      className={`inline-flex items-center gap-2 rounded-full border px-3.5 py-2 text-[13px] font-semibold transition-colors ${on ? active : 'border-white/[0.08] bg-[#15151A] text-white/65'}`}
    >
      {dot ? <span className="h-[7px] w-[7px] rounded-full" style={{ background: dot }} /> : null}
      {label}
      {count != null ? (
        <span className="tabular-nums opacity-60">{count.toLocaleString()}</span>
      ) : null}
    </button>
  );
}

function Pager({
  dir,
  disabled,
  onClick,
  label,
}: Readonly<{ dir: 'prev' | 'next'; disabled: boolean; onClick: () => void; label: string }>) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`inline-flex items-center gap-1.5 rounded-lg border px-3 py-2 text-[12.5px] font-semibold ${disabled ? 'cursor-not-allowed border-white/[0.06] bg-[#141419] text-white/[0.28]' : 'border-white/12 bg-[#1A1A20] text-white/80'}`}
    >
      {dir === 'prev' ? <IconChevronLeft size={13} stroke={2.6} /> : null}
      {label}
      {dir === 'next' ? <IconChevronRight size={13} stroke={2.6} /> : null}
    </button>
  );
}
