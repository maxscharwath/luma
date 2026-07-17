// Slide-in request drawer: poster + identity, requester, seasons, status,
// denial note / failure detail, and the moderation actions (approve / deny
// with optional reason / delete). The interactive release search joins with
// the indexer milestone.

import {
  apiErrorText,
  type InteractiveSearchView,
  type MediaRequest,
  type MessageKey,
  type ScoredReleaseView,
} from '@kroma/core';
import { useModuleEnabled } from '@kroma/admin-kit';
import { useT } from '@kroma/ui';
import { IconCheck, IconLoader2, IconSearch, IconTrash, IconX } from '@tabler/icons-react';
import { useEffect, useState } from 'react';
import { kindMeta, posterGrad } from '#web/features/admin/pipeline-meta';
import { ReleaseList } from '#web/features/admin/release-list';
import { Avatar } from '#web/features/admin/ui';
import { RequestStatusChip } from '#web/features/requests/request-status-chip';
import { seasonsSummary } from '#web/features/requests/status';
import { useAuth } from '#web/shared/lib/auth';

function DrawerPoster({ req }: Readonly<{ req: MediaRequest }>) {
  const [broken, setBroken] = useState(false);
  return (
    <div
      className="relative h-[104px] w-[70px] flex-[0_0_70px] overflow-hidden rounded-[10px] shadow-[0_10px_24px_rgba(0,0,0,.5)]"
      style={{ background: posterGrad(req.title) }}
    >
      {req.posterUrl && !broken ? (
        <img
          src={req.posterUrl}
          alt=""
          onError={() => setBroken(true)}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : null}
    </div>
  );
}

export function RequestDrawer({
  req,
  busy,
  canReview,
  onClose,
  onApprove,
  onDeny,
  onDelete,
}: Readonly<{
  req: MediaRequest | null;
  busy: boolean;
  canReview: boolean;
  onClose: () => void;
  onApprove: () => void;
  onDeny: (note: string) => void;
  onDelete: () => void;
}>) {
  const t = useT();
  const { client } = useAuth();
  // The interactive release search + grab are the Acquisition module's feature;
  // hide the whole panel when it is disabled (its routes 404 too).
  const acqEnabled = useModuleEnabled('tv.kroma.acquisition');
  const open = !!req;
  const km = kindMeta(req?.kind === 'show' ? 'series' : 'film');
  const [denying, setDenying] = useState(false);
  const [note, setNote] = useState('');
  const [search, setSearch] = useState<{
    busy: boolean;
    view: InteractiveSearchView | null;
    error: string | null;
  }>({ busy: false, view: null, error: null });

  // Reset the deny form + search results whenever another request opens. `req?.id`
  // is read only in the dep array on purpose: it keys the reset to the open
  // request, so removing it would stop the form clearing between requests.
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional re-run key; req?.id gates the reset to each opened request
  useEffect(() => {
    setDenying(false);
    setNote('');
    setSearch({ busy: false, view: null, error: null });
  }, [req?.id]);

  const [grabbed, setGrabbed] = useState<{ title: string; error: boolean } | null>(null);
  const runSearch = () => {
    if (!req) return;
    setGrabbed(null);
    setSearch({ busy: true, view: null, error: null });
    client
      .searchReleases(req.id)
      .then((view) => setSearch({ busy: false, view, error: null }))
      .catch((e) =>
        setSearch({ busy: false, view: null, error: apiErrorText(e, t('requests.searchFailed')) }),
      );
  };
  const grab = (release: ScoredReleaseView) => {
    if (!req) return;
    client
      .grabRelease(req.id, { guid: release.guid, indexerId: release.indexerId })
      .then(() => setGrabbed({ title: release.title, error: false }))
      .catch((e) =>
        setGrabbed({ title: apiErrorText(e, t('requests.actionFailed')), error: true }),
      );
  };

  const seasons = req ? seasonsSummary(req.seasons) : null;

  return (
    <>
      <button
        type="button"
        aria-label={t('common.close')}
        onClick={onClose}
        className={`fixed inset-0 z-[60] bg-[rgba(4,4,6,.6)] backdrop-blur-[2px] transition-opacity ${open ? 'opacity-100' : 'pointer-events-none opacity-0'}`}
      />
      <aside
        className="fixed right-0 top-0 z-[61] flex h-screen w-[460px] max-w-full flex-col border-l border-white/[0.09] bg-[#0E0E12] shadow-[-20px_0_60px_rgba(0,0,0,.6)] transition-transform duration-300 ease-[cubic-bezier(.22,1,.36,1)] sm:max-w-[92vw]"
        style={{ transform: open ? 'translateX(0)' : 'translateX(105%)' }}
      >
        {req ? (
          <>
            <div className="border-b border-white/[0.07] px-6 py-5">
              <div className="mb-4 flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
                  {t('requests.sheet')}
                </span>
                <button type="button" onClick={onClose} className="text-white/60 hover:text-white">
                  <IconX size={20} stroke={2.1} />
                </button>
              </div>
              <div className="flex gap-4">
                <DrawerPoster req={req} />
                <div className="min-w-0 pt-1">
                  <span
                    className="rounded-full px-[9px] py-[3px] text-[9.5px] font-bold uppercase tracking-[.1em]"
                    style={{ color: km.color, background: km.bg }}
                  >
                    {t(`pipeline.type.${km.typeKey}` as MessageKey)}
                  </span>
                  <h2 className="mt-2.5 font-display text-[21px] font-bold leading-[1.12]">
                    {req.title}
                  </h2>
                  <div className="mt-1.5 text-[12.5px] font-medium text-white/50">
                    {[
                      req.year ? String(req.year) : '',
                      req.kind === 'show' ? (seasons ?? t('requests.allSeasons')) : '',
                    ]
                      .filter(Boolean)
                      .join(' · ')}
                  </div>
                  <div className="mt-2.5">
                    <RequestStatusChip status={req.status} />
                  </div>
                </div>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto px-6 py-5">
              <div className="mb-3 text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
                {t('requests.requestedBy')}
              </div>
              <div className="flex items-center gap-3 rounded-xl border border-white/[0.07] bg-[#121216] px-4 py-3.5">
                <Avatar name={req.requestedByName ?? '?'} size={34} />
                <div className="min-w-0">
                  <div className="truncate text-[14px] font-bold">
                    {req.requestedByName ?? t('requests.unknownUser')}
                  </div>
                  <div className="text-[12px] font-medium text-white/45">
                    {new Date(req.createdAt).toLocaleDateString()}{' '}
                    {new Date(req.createdAt).toLocaleTimeString([], {
                      hour: '2-digit',
                      minute: '2-digit',
                    })}
                  </div>
                </div>
              </div>

              {req.note ? (
                <div className="mt-4 rounded-lg border border-[#E8536A]/[0.18] bg-[#E8536A]/[0.08] px-[11px] py-2.5 text-[12.5px] leading-[1.45] text-[#EF8091]">
                  {req.note}
                </div>
              ) : null}

              {acqEnabled && canReview && req.status !== 'denied' && req.status !== 'available' ? (
                <div className="mt-5">
                  <div className="mb-3 flex items-center justify-between">
                    <span className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
                      {t('requests.interactiveSearch')}
                    </span>
                    <button
                      type="button"
                      onClick={runSearch}
                      disabled={search.busy}
                      className="inline-flex items-center gap-1.5 rounded-lg border border-white/12 bg-[#1A1A20] px-3 py-1.5 text-[12px] font-semibold text-white/80 hover:bg-[#222229] disabled:opacity-60"
                    >
                      {search.busy ? (
                        <IconLoader2 size={12} stroke={2.4} className="animate-spin" />
                      ) : (
                        <IconSearch size={12} stroke={2.4} />
                      )}
                      {t(search.busy ? 'requests.searching2' : 'requests.searchNow')}
                    </button>
                  </div>
                  {search.error ? (
                    <div className="rounded-lg border border-[#E8536A]/[0.18] bg-[#E8536A]/[0.08] px-3 py-2 text-[12px] font-semibold text-[#EF8091]">
                      {search.error}
                    </div>
                  ) : null}
                  {grabbed ? (
                    <div
                      className={`mb-2 rounded-lg border px-3 py-2 text-[12px] font-semibold ${grabbed.error ? 'border-[#E8536A]/[0.18] bg-[#E8536A]/[0.08] text-[#EF8091]' : 'border-[#46D08D]/25 bg-[#46D08D]/[0.09] text-[#46D08D]'}`}
                    >
                      {grabbed.error ? grabbed.title : `${t('requests.grabbed')} ${grabbed.title}`}
                    </div>
                  ) : null}
                  {search.view ? (
                    <ReleaseList
                      releases={search.view.releases}
                      errors={search.view.indexerErrors}
                      canGrab={canReview}
                      busy={busy}
                      onGrab={grab}
                    />
                  ) : null}
                </div>
              ) : null}
            </div>

            {canReview ? (
              <div className="border-t border-white/[0.07] px-6 py-4.5">
                {denying ? (
                  <div className="flex flex-col gap-2.5">
                    <input
                      value={note}
                      onChange={(e) => setNote(e.target.value)}
                      placeholder={t('requests.denyNote')}
                      className="w-full rounded-xl border border-white/12 bg-[#15151A] px-3.5 py-3 text-[13.5px] font-medium text-white outline-none placeholder:text-white/35 focus:border-white/25"
                    />
                    <div className="flex gap-2.5">
                      <button
                        type="button"
                        disabled={busy}
                        onClick={() => onDeny(note.trim())}
                        className="flex flex-1 items-center justify-center gap-2 rounded-xl bg-[#E8536A] px-4 py-3 text-[13.5px] font-bold text-white transition-colors hover:bg-[#EF8091] disabled:opacity-60"
                      >
                        {busy ? (
                          <IconLoader2 size={15} stroke={2.4} className="animate-spin" />
                        ) : (
                          <IconX size={15} stroke={2.6} />
                        )}
                        {t('requests.confirmDeny')}
                      </button>
                      <button
                        type="button"
                        onClick={() => setDenying(false)}
                        className="rounded-xl border border-white/12 bg-[#1A1A20] px-4 py-3 text-[13.5px] font-semibold text-white/75"
                      >
                        {t('common.cancel')}
                      </button>
                    </div>
                  </div>
                ) : (
                  <div className="flex gap-2.5">
                    {req.status === 'pending' || req.status === 'failed' ? (
                      <button
                        type="button"
                        disabled={busy}
                        onClick={onApprove}
                        className="flex flex-1 items-center justify-center gap-2 rounded-xl bg-accent px-4 py-3 text-[13.5px] font-bold text-[#0A0A0C] transition-colors hover:bg-accent-hover disabled:opacity-60"
                      >
                        {busy ? (
                          <IconLoader2 size={15} stroke={2.4} className="animate-spin" />
                        ) : (
                          <IconCheck size={15} stroke={2.8} />
                        )}
                        {t(req.status === 'failed' ? 'requests.retry' : 'requests.approve')}
                      </button>
                    ) : null}
                    {req.status === 'pending' ? (
                      <button
                        type="button"
                        onClick={() => setDenying(true)}
                        className="flex flex-1 items-center justify-center gap-2 rounded-xl border border-[#E8536A]/35 bg-[#E8536A]/10 px-4 py-3 text-[13.5px] font-bold text-[#E8536A] transition-colors hover:bg-[#E8536A]/20"
                      >
                        <IconX size={15} stroke={2.6} />
                        {t('requests.deny')}
                      </button>
                    ) : null}
                    <button
                      type="button"
                      disabled={busy}
                      onClick={onDelete}
                      title={t('requests.delete')}
                      className="flex h-[46px] w-[46px] flex-[0_0_46px] items-center justify-center rounded-xl border border-white/12 bg-[#1A1A20] text-white/60 transition-colors hover:text-[#E8536A]"
                    >
                      <IconTrash size={16} stroke={2} />
                    </button>
                  </div>
                )}
              </div>
            ) : null}
          </>
        ) : null}
      </aside>
    </>
  );
}
