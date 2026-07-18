// Admin "Pipeline de traitement" console, element-centric: the whole catalog as
// a searchable / filterable table where each film, series and episode shows the
// status of every treatment applied to it, with a detail drawer to inspect and
// reprocess. Backed by GET /api/admin/pipeline/elements + the pipeline.stats WS.

import { type ElementRow, type KromaClient, KromaEvents, type MessageKey } from '@kroma/core';
import { useT } from '@kroma/ui';
import {
  IconChevronLeft,
  IconChevronRight,
  IconInbox,
  IconPlayerPause,
  IconPlayerPlay,
} from '@tabler/icons-react';
import { type Dispatch, type SetStateAction, useEffect, useState } from 'react';
import { PipelineDrawer } from '#web/features/admin/pipeline-drawer';
import { ElementRowView } from '#web/features/admin/pipeline-row';
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
import { EmptyState } from '#web/shared/ui';

const PER_PAGE = 30;
const apiKind = (el: ElementRow): 'item' | 'show' => (el.kind === 'series' ? 'show' : 'item');

// Server events that should refresh the pipeline table.
const RELOAD_EVENTS = new Set([
  'pipeline.stats',
  'job.finished',
  'job.started',
  'item.updated',
  'show.updated',
  'library.updated',
]);

/** Refetch the table whenever a pipeline-relevant server event lands. */
function usePipelineReloadEvents(onReload: () => void): void {
  useEffect(() => {
    const ev = new KromaEvents(apiBase(), {
      onEvent: (e) => {
        if (RELOAD_EVENTS.has(e.type)) onReload();
      },
    });
    ev.connect();
    return () => ev.close();
  }, [onReload]);
}

/** Keep the open drawer in sync with a fresh reload while its element is on the
 * page. Pulled out of the effect so the page body stays flat. */
function syncOpenDrawer(
  drawer: ElementRow | null,
  data: { elements: ElementRow[] } | null | undefined,
  setDrawer: Dispatch<SetStateAction<ElementRow | null>>,
): void {
  if (!drawer || !data) return;
  const fresh = data.elements.find((e) => e.id === drawer.id);
  if (fresh) setDrawer(fresh);
}

/** The three privileged pipeline actions (pause toggle, reprocess a subject,
 * retry one stage). Built fresh each render like the inline handlers it replaces,
 * but out of the page body so its cognitive complexity stays low. */
function pipelineActions(deps: {
  client: KromaClient;
  canManage: boolean;
  t: ReturnType<typeof useT>;
  reload: () => void;
  paused: boolean;
  setPaused: Dispatch<SetStateAction<boolean>>;
  setBusy: Dispatch<SetStateAction<boolean>>;
  flash: (text: string) => void;
}) {
  const { client, canManage, t, reload, paused, setPaused, setBusy, flash } = deps;
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
        const stageName = t(`pipeline.t.${stage}` as MessageKey);
        flash(`${stageName} ${t('pipeline.toastRetry')}`);
        reload();
      })
      .finally(() => setBusy(false));
  };
  return { togglePause, reprocess, retryStage };
}

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
  const { toast, flash } = useConsoleToast();

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
  const throttledReload = useThrottledReload(reload);

  usePipelineReloadEvents(throttledReload);

  // Keep the open drawer fresh from reloads while its element is on the page.
  useEffect(() => syncOpenDrawer(drawer, data, setDrawer), [data, drawer]);

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

  const { togglePause, reprocess, retryStage } = pipelineActions({
    client,
    canManage,
    t,
    reload,
    paused,
    setPaused,
    setBusy,
    flash,
  });

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
    <>
      <PageHeader
        title={t('admin.pipelineTitle')}
        action={
          <div className="flex min-w-0 flex-wrap items-center gap-3">
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
            <ConsoleSearch
              value={q}
              onChange={setQ}
              placeholder={t('pipeline.searchPlaceholder')}
            />
          </div>
        }
      />
      <ConsoleSummary
        total={total}
        totalLabel={t('pipeline.trackedLabel')}
        accent={attention}
        accentLabel={t('pipeline.needActionLabel')}
      />

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
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b border-white/[0.06] bg-[#15151A] px-5 py-3 md:grid-cols-[minmax(0,1fr)_150px_132px_46px]">
          <Head>{t('pipeline.colElement')}</Head>
          <Head className="max-md:hidden">{t('pipeline.treatments')}</Head>
          <Head className="max-md:hidden">{t('pipeline.colStatus')}</Head>
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
          <div className="py-6">
            <EmptyState icon={<IconInbox size={32} stroke={1.5} />} title={t('pipeline.noMatch')} />
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

      <ConsoleToast toast={toast} />
    </>
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
