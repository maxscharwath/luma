// Slide-in report drawer: subject identity (category + status + kind), the
// reporter + date, the free-text message, a deep-link to the title's fiche, and
// the triage actions (resolve / dismiss / reopen / delete).

import type { Report, ReportStatus } from '@kroma/core';
import { useT } from '@kroma/ui';
import {
  IconArrowBackUp,
  IconCheck,
  IconExternalLink,
  IconLoader2,
  IconTrash,
  IconX,
} from '@tabler/icons-react';
import { useNavigate } from '@tanstack/react-router';
import { useEffect, useState } from 'react';
import { createCallable } from 'react-call';
import { categoryMeta, kindLabelKey, soft, statusMeta } from '#web/features/admin/report-meta';
import { Avatar } from '#web/features/admin/ui';

function Header({ report, onClose }: Readonly<{ report: Report; onClose: () => void }>) {
  const t = useT();
  const cat = categoryMeta(report.category);
  const st = statusMeta(report.status);
  return (
    <div className="border-b border-white/[0.07] px-6 py-5">
      <div className="mb-4 flex items-center justify-between">
        <span className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
          {t('reports.sheet')}
        </span>
        <button type="button" onClick={onClose} className="text-white/60 hover:text-white">
          <IconX size={20} stroke={2.1} />
        </button>
      </div>
      <div className="mb-2.5 flex flex-wrap items-center gap-2">
        <span
          className="rounded-full px-[9px] py-[3px] text-[9.5px] font-bold uppercase tracking-widest"
          style={{ color: cat.color, background: soft(cat.color) }}
        >
          {t(cat.labelKey)}
        </span>
        <span
          className="rounded-full px-[9px] py-[3px] text-[9.5px] font-bold uppercase tracking-widest"
          style={{ color: st.color, background: soft(st.color) }}
        >
          {t(st.labelKey)}
        </span>
        <span className="text-[11px] font-semibold uppercase tracking-wide text-white/40">
          {t(kindLabelKey(report.subjectKind))}
        </span>
      </div>
      <h2 className="font-display text-[21px] font-bold leading-[1.15]">{report.subjectTitle}</h2>
    </div>
  );
}

/**
 * Slide-in triage drawer, as an imperative callable: open it with
 * `await ReportDrawer.call({ report, canManage, onResolve, ... })`. The action
 * callbacks (each performs the mutation + parent list refresh + toast) are
 * passed in so the queue keeps updating live while the drawer stays open; the
 * drawer owns its own busy + report state and resolves `void` when it closes.
 * Its root is mounted once by `AdminModalHosts`; no open-state at the call site.
 */
export const ReportDrawer = createCallable<
  {
    report: Report;
    canManage: boolean;
    onResolve: (r: Report) => Promise<void>;
    onDismiss: (r: Report) => Promise<void>;
    onReopen: (r: Report) => Promise<void>;
    onDelete: (r: Report) => Promise<void>;
  },
  void
>(({ call, report: initial, canManage, onResolve, onDismiss, onReopen, onDelete }) => {
  const t = useT();
  const navigate = useNavigate();
  const [report, setReport] = useState(initial);
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState(false);

  // react-call mounts us on `.call()` / unmounts on `call.end()`, so drive the
  // slide with a mount effect (in) and a delayed end (out) to keep the anim.
  useEffect(() => {
    const id = requestAnimationFrame(() => setOpen(true));
    return () => cancelAnimationFrame(id);
  }, []);
  const close = () => {
    setOpen(false);
    window.setTimeout(() => call.end(), 300);
  };

  // A triage action: disable the drawer while it runs, then reflect the new
  // status locally (the parent reloads the list behind us). Failures leave the
  // report untouched (the callback already surfaced the error toast).
  const run = (fn: (r: Report) => Promise<void>, next: ReportStatus) => {
    setBusy(true);
    fn(report)
      .then(() => setReport({ ...report, status: next }))
      .catch(() => {})
      .finally(() => setBusy(false));
  };
  const del = () => {
    void onDelete(report);
    close();
  };

  // Movies + shows have a fiche route; an episode item has no standalone page.
  const FICHE_ROUTES = { movie: '/movie/$id', show: '/show/$id' } as const;
  const ficheTo =
    report.subjectKind === 'movie' || report.subjectKind === 'show'
      ? FICHE_ROUTES[report.subjectKind]
      : null;

  return (
    <>
      <button
        type="button"
        aria-label={t('common.close')}
        onClick={close}
        className={`fixed inset-0 z-60 bg-[rgba(4,4,6,.6)] backdrop-blur-[2px] transition-opacity ${open ? 'opacity-100' : 'pointer-events-none opacity-0'}`}
      />
      <aside
        className="fixed right-0 top-0 z-61 flex h-screen w-[460px] max-w-full flex-col border-l border-white/9 bg-[#0E0E12] shadow-[-20px_0_60px_rgba(0,0,0,.6)] transition-transform duration-300 ease-out sm:max-w-[92vw]"
        style={{ transform: open ? 'translateX(0)' : 'translateX(105%)' }}
      >
        <Header report={report} onClose={close} />

        <div className="flex-1 overflow-y-auto px-6 py-5">
          <div className="mb-3 text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
            {t('reports.reportedBy')}
          </div>
          <div className="flex items-center gap-3 rounded-xl border border-white/[0.07] bg-[#121216] px-4 py-3.5">
            <Avatar name={report.reportedByName ?? '?'} size={34} />
            <div className="min-w-0">
              <div className="truncate text-[14px] font-bold">
                {report.reportedByName ?? t('reports.unknownUser')}
              </div>
              <div className="text-[12px] font-medium text-white/45">
                {new Date(report.createdAt).toLocaleDateString()}{' '}
                {new Date(report.createdAt).toLocaleTimeString([], {
                  hour: '2-digit',
                  minute: '2-digit',
                })}
              </div>
            </div>
          </div>

          {report.message ? (
            <div className="mt-4 whitespace-pre-wrap rounded-xl border border-white/[0.07] bg-[#121216] px-4 py-3.5 text-[13.5px] leading-[1.5] text-white/80">
              {report.message}
            </div>
          ) : (
            <p className="mt-4 text-[13px] italic text-white/35">{t('reports.noMessage')}</p>
          )}

          {ficheTo ? (
            <button
              type="button"
              onClick={() => navigate({ to: ficheTo, params: { id: report.subjectId } })}
              className="mt-4 inline-flex items-center gap-2 rounded-xl border border-white/12 bg-[#1A1A20] px-3.5 py-2.5 text-[13px] font-semibold text-white/80 transition-colors hover:bg-[#222229]"
            >
              <IconExternalLink size={15} stroke={2} />
              {t('reports.viewTitle')}
            </button>
          ) : null}
        </div>

        {canManage ? (
          <div className="flex gap-2.5 border-t border-white/[0.07] px-6 py-4.5">
            {report.status === 'open' ? (
              <>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => run(onResolve, 'resolved')}
                  className="flex flex-1 items-center justify-center gap-2 rounded-xl bg-accent px-4 py-3 text-[13.5px] font-bold text-[#0A0A0C] transition-colors hover:bg-accent-hover disabled:opacity-60"
                >
                  {busy ? (
                    <IconLoader2 size={15} stroke={2.4} className="animate-spin" />
                  ) : (
                    <IconCheck size={15} stroke={2.8} />
                  )}
                  {t('reports.actionResolve')}
                </button>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => run(onDismiss, 'dismissed')}
                  className="flex flex-1 items-center justify-center gap-2 rounded-xl border border-white/12 bg-[#1A1A20] px-4 py-3 text-[13.5px] font-bold text-white/75 transition-colors hover:bg-[#222229] disabled:opacity-60"
                >
                  <IconX size={15} stroke={2.6} />
                  {t('reports.actionDismiss')}
                </button>
              </>
            ) : (
              <button
                type="button"
                disabled={busy}
                onClick={() => run(onReopen, 'open')}
                className="flex flex-1 items-center justify-center gap-2 rounded-xl border border-white/12 bg-[#1A1A20] px-4 py-3 text-[13.5px] font-bold text-white/85 transition-colors hover:bg-[#222229] disabled:opacity-60"
              >
                <IconArrowBackUp size={15} stroke={2.4} />
                {t('reports.actionReopen')}
              </button>
            )}
            <button
              type="button"
              disabled={busy}
              onClick={del}
              title={t('reports.actionDelete')}
              className="flex h-[46px] w-[46px] flex-[0_0_46px] items-center justify-center rounded-xl border border-white/12 bg-[#1A1A20] text-white/60 transition-colors hover:text-[#E8536A]"
            >
              <IconTrash size={16} stroke={2} />
            </button>
          </div>
        ) : null}
      </aside>
    </>
  );
});
