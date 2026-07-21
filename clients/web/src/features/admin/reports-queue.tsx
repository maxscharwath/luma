// Admin "Signalements" queue: every user-submitted problem report as a
// searchable / filterable table (mirroring the Demandes queue), with a triage
// drawer. Backed by GET /api/admin/reports + the report.updated WS event.

import { KromaEvents, type Report, type ReportCategory, type ReportStatus } from '@kroma/core';
import { useT } from '@kroma/ui';
import { IconFlag } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { ReportDrawer } from '#web/features/admin/report-drawer';
import { categoryMeta, kindLabelKey, soft, statusMeta } from '#web/features/admin/report-meta';
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
import { Avatar } from '#web/features/admin/ui';
import { apiBase } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { EmptyState, TableSkeleton } from '#web/shared/ui';

type StatusBucket = ReportStatus | 'all';
type CategoryFilter = ReportCategory | 'all';

const CATEGORIES: ReportCategory[] = ['metadata', 'video', 'audio', 'subtitles', 'other'];

function ReportRow({ report, onOpen }: Readonly<{ report: Report; onOpen: () => void }>) {
  const t = useT();
  const cat = categoryMeta(report.category);
  const st = statusMeta(report.status);
  return (
    <button
      type="button"
      onClick={onOpen}
      className="grid w-full cursor-pointer grid-cols-[minmax(0,1fr)_auto] items-center gap-4 border-b border-white/4 px-5 py-3 text-left transition-colors hover:bg-white/[0.028] md:grid-cols-[minmax(0,1fr)_128px_150px_96px_116px]"
    >
      <div className="min-w-0">
        <div className="truncate text-[14.5px] font-bold">{report.subjectTitle}</div>
        <div className="mt-[3px] truncate text-[12px] font-medium text-white/50">
          {t(kindLabelKey(report.subjectKind))}
          {report.message ? ` · ${report.message}` : ''}
        </div>
      </div>

      <span className="max-md:hidden">
        <span
          className="rounded-full px-[9px] py-1 text-[10px] font-bold uppercase tracking-wide"
          style={{ color: cat.color, background: soft(cat.color) }}
        >
          {t(cat.labelKey)}
        </span>
      </span>

      <div className="flex min-w-0 items-center gap-2.5 max-md:hidden">
        <Avatar name={report.reportedByName ?? '?'} size={26} />
        <span className="truncate text-[13px] font-semibold text-white/75">
          {report.reportedByName ?? t('reports.unknownUser')}
        </span>
      </div>

      <span className="text-[12.5px] font-semibold tabular-nums text-white/55 max-md:hidden">
        {new Date(report.createdAt).toLocaleDateString()}
      </span>

      <span className="max-md:hidden">
        <span
          className="inline-flex items-center gap-1.5 rounded-full px-[9px] py-1 text-[11px] font-bold"
          style={{ color: st.color, background: soft(st.color) }}
        >
          <span className="h-[6px] w-[6px] rounded-full" style={{ background: st.color }} />
          {t(st.labelKey)}
        </span>
      </span>
    </button>
  );
}

export function ReportsQueuePage() {
  const t = useT();
  const { client } = useAuth();
  const canManage = useCap('reports.manage');

  const [status, setStatus] = useState<StatusBucket>('open');
  const [category, setCategory] = useState<CategoryFilter>('all');
  const [q, setQ] = useState('');
  const { toast, flash } = useConsoleToast();

  const { data, reload } = usePoll(['admin', 'reports'], () => client.adminReports(), 30000);

  const throttledReload = useThrottledReload(reload);
  useEffect(() => {
    const ev = new KromaEvents(apiBase(), {
      onEvent: (e) => {
        if (e.type === 'report.updated') throttledReload();
      },
    });
    ev.connect();
    return () => ev.close();
  }, [throttledReload]);

  // Runs a triage mutation, then refreshes the list + toasts on success (the
  // drawer owns its own busy state and awaits this). Rejects on failure so the
  // drawer can leave the report untouched, after surfacing the error toast.
  const act = (label: string, fn: () => Promise<unknown>) =>
    fn()
      .then(() => {
        flash(label);
        reload();
      })
      .catch((e) => {
        flash(t('reports.actionFailed'));
        throw e;
      });
  const resolve = (r: Report) =>
    act(`« ${r.subjectTitle} » ${t('reports.toastResolved')}`, () => client.resolveReport(r.id));
  const dismiss = (r: Report) =>
    act(`« ${r.subjectTitle} » ${t('reports.toastDismissed')}`, () => client.dismissReport(r.id));
  const reopen = (r: Report) =>
    act(`« ${r.subjectTitle} » ${t('reports.toastReopened')}`, () => client.reopenReport(r.id));
  const remove = (r: Report) =>
    act(`« ${r.subjectTitle} » ${t('reports.toastDeleted')}`, () => client.deleteReport(r.id));

  const openDrawer = (report: Report) =>
    ReportDrawer.call({
      report,
      canManage,
      onResolve: resolve,
      onDismiss: dismiss,
      onReopen: reopen,
      onDelete: remove,
    });

  const all = data?.reports ?? [];
  const c = data?.counts;
  const needle = q.trim().toLowerCase();
  const rows = all.filter(
    (r) =>
      (status === 'all' || r.status === status) &&
      (category === 'all' || r.category === category) &&
      (!needle ||
        r.subjectTitle.toLowerCase().includes(needle) ||
        (r.reportedByName ?? '').toLowerCase().includes(needle)),
  );
  const catCount = (cat: ReportCategory) => all.filter((r) => r.category === cat).length;

  return (
    <>
      <PageHeader
        title={t('admin.reportsTitle')}
        action={
          <ConsoleSearch value={q} onChange={setQ} placeholder={t('reports.searchPlaceholder')} />
        }
      />
      <ConsoleSummary
        total={c?.total ?? 0}
        totalLabel={t('reports.totalLabel')}
        accent={c?.open ?? 0}
        accentLabel={t('reports.openLabel')}
      />

      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <Chip
          label={t('reports.status.open')}
          count={c?.open}
          dot="#F4B642"
          on={status === 'open'}
          onClick={() => setStatus('open')}
        />
        <Chip
          label={t('reports.status.resolved')}
          count={c?.resolved}
          dot="#46D08D"
          on={status === 'resolved'}
          onClick={() => setStatus('resolved')}
        />
        <Chip
          label={t('reports.status.dismissed')}
          count={c?.dismissed}
          dot="#9AA0AA"
          on={status === 'dismissed'}
          onClick={() => setStatus('dismissed')}
        />
        <Chip
          label={t('reports.filterAll')}
          count={c?.total}
          on={status === 'all'}
          onClick={() => setStatus('all')}
        />
      </div>

      <div className="mb-4 flex flex-wrap items-center gap-2">
        <Chip
          label={t('reports.filterAll')}
          on={category === 'all'}
          tone="blue"
          onClick={() => setCategory('all')}
        />
        {CATEGORIES.map((cat) => (
          <Chip
            key={cat}
            label={t(categoryMeta(cat).labelKey)}
            count={catCount(cat)}
            tone="blue"
            on={category === cat}
            onClick={() => setCategory(cat)}
          />
        ))}
      </div>

      <div className="overflow-hidden rounded-2xl border border-white/8 bg-[#121216] shadow-[0_10px_28px_rgba(0,0,0,.3)]">
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b border-white/6 bg-[#15151A] px-5 py-3 md:grid-cols-[minmax(0,1fr)_128px_150px_96px_116px]">
          <Head>{t('reports.colTitle')}</Head>
          <Head className="max-md:hidden">{t('reports.colCategory')}</Head>
          <Head className="max-md:hidden">{t('reports.colReporter')}</Head>
          <Head className="max-md:hidden">{t('reports.colDate')}</Head>
          <Head className="max-md:hidden">{t('reports.colStatus')}</Head>
        </div>

        {rows.map((r) => (
          <ReportRow key={r.id} report={r} onOpen={() => void openDrawer(r)} />
        ))}

        {data === null ? <TableSkeleton rows={8} /> : null}

        {data && rows.length === 0 ? (
          <div className="py-6">
            <EmptyState
              icon={<IconFlag size={32} stroke={1.5} />}
              title={all.length === 0 ? t('reports.empty') : t('reports.noMatch')}
            />
          </div>
        ) : null}
      </div>

      <ConsoleToast toast={toast} />
    </>
  );
}
