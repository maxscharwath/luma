// Admin "Téléchargements": the live download queue (progress fed by a
// page-scoped download.progress stream, slow poll as the safety net), a VPN
// status banner, aggregate stat cards and the download-clients section.

import { type DownloadView, LumaEvents } from '@luma/core';
import { useT } from '@luma/ui';
import {
  IconDownload,
  IconPlayerPause,
  IconPlayerPlay,
  IconShieldCheck,
  IconShieldX,
  IconUsersPlus,
} from '@tabler/icons-react';
import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react';
import { DownloadClientsSection } from '#web/features/admin/download-clients';
import { DownloadRowView, type LiveDl } from '#web/features/admin/download-row';
import { ManualGrabModal } from '#web/features/admin/manual-grab';
import { useCap, usePoll } from '#web/features/admin/shell';
import { Modal, ModalActions, StatCard } from '#web/features/admin/ui';
import { VpnCard } from '#web/features/admin/vpn-card';
import { formatBytes } from '#web/shared/lib/adminFormat';
import { apiBase } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';
import { TableSkeleton } from '#web/shared/ui';

export function DownloadsPage() {
  const t = useT();
  const { client } = useAuth();
  const canSettings = useCap('settings.manage');
  const canQueue = useCap('requests.manage') || canSettings;

  const [live, setLive] = useState<Record<string, LiveDl>>({});
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState<DownloadView | null>(null);
  const [wipeData, setWipeData] = useState(true);
  const [manual, setManual] = useState(false);

  // Slow poll = reconnect/missed-event safety net; progress rides the WS.
  const { data, reload } = usePoll(['admin', 'downloads'], () => client.adminDownloads(), 10000);

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
        if (e.type === 'download.progress') {
          setLive((s) => ({
            ...s,
            [e.id]: {
              progress: e.progress,
              downBps: e.downBps,
              upBps: e.upBps,
              peers: e.peers,
              peersSeen: e.peersSeen,
              state: e.state,
            },
          }));
        } else if (
          e.type === 'download.completed' ||
          e.type === 'request.updated' ||
          e.type === 'vpn.status'
        ) {
          throttledReload();
        }
      },
    });
    ev.connect();
    return () => ev.close();
  }, [throttledReload]);

  const act = (fn: () => Promise<unknown>) => {
    setBusy(true);
    fn()
      .catch(() => undefined)
      .finally(() => {
        setBusy(false);
        reload();
      });
  };

  const downloads = data?.downloads ?? [];
  const activeRows = downloads.filter((d) =>
    ['queued', 'downloading', 'seeding', 'paused'].includes(d.status),
  );
  const doneRows = downloads.filter(
    (d) => !['queued', 'downloading', 'seeding', 'paused'].includes(d.status),
  );
  const totalDown = Object.entries(live)
    .filter(([id]) => activeRows.some((d) => d.id === id))
    .reduce((sum, [, l]) => sum + l.downBps, 0);
  const totalUp = Object.entries(live)
    .filter(([id]) => activeRows.some((d) => d.id === id))
    .reduce((sum, [, l]) => sum + l.upBps, 0);
  const vpn = data?.vpn ?? null;

  if (!canQueue) return null;

  return (
    <div className="min-w-0 max-w-[1280px]">
      <div className="mb-5 flex flex-wrap items-start justify-between gap-6">
        <div>
          <h1 className="font-display text-[clamp(26px,5vw,34px)] font-bold leading-[1.05] tracking-[-.02em]">
            {t('admin.downloadsTitle')}
          </h1>
          <p className="mt-2 text-[14.5px] font-medium text-white/50">{t('admin.downloadsSub')}</p>
        </div>
        <button
          type="button"
          onClick={() => setManual(true)}
          className="inline-flex shrink-0 items-center gap-2 rounded-xl bg-accent px-4.5 py-2.75 text-[14px] font-bold text-accent-ink transition-colors hover:bg-accent-hover"
        >
          <IconDownload size={16} stroke={2.4} />
          {t('manual.title')}
        </button>
      </div>

      {canSettings ? <VpnCard /> : null}

      {vpn ? (
        <div
          className={`mb-4 flex items-center gap-2.5 rounded-xl border px-4 py-2.5 text-[13.5px] font-semibold ${
            vpn.connected
              ? 'border-[#46D08D]/30 bg-[#46D08D]/[0.10] text-[#46D08D]'
              : 'border-[#F4B642]/30 bg-[#F4B642]/[0.10] text-[#F4B642]'
          }`}
        >
          {vpn.connected ? (
            <IconShieldCheck size={15} stroke={2} />
          ) : (
            <IconShieldX size={15} stroke={2} />
          )}
          {vpn.connected
            ? t('downloads.vpnOk', { ip: vpn.exitIp ?? '?' })
            : vpn.paused
              ? t('downloads.vpnBlocked')
              : t('downloads.vpnDown')}
        </div>
      ) : null}

      <div className="mb-5 grid grid-cols-2 gap-4 lg:grid-cols-4">
        <StatCard label={t('downloads.statActive')} value={String(activeRows.length)} />
        <StatCard label={t('downloads.statDown')} value={`${formatBytes(totalDown)}/s`} />
        <StatCard label={t('downloads.statUp')} value={`${formatBytes(totalUp)}/s`} />
        <StatCard label={t('downloads.statHistory')} value={String(doneRows.length)} />
      </div>

      {activeRows.length > 0 ? (
        <div className="mb-3 flex flex-wrap items-center gap-2">
          <BulkBtn onClick={() => act(() => client.pauseAllDownloads())} busy={busy}>
            <IconPlayerPause size={14} stroke={2.2} />
            {t('downloads.pauseAll')}
          </BulkBtn>
          <BulkBtn onClick={() => act(() => client.resumeAllDownloads())} busy={busy}>
            <IconPlayerPlay size={14} stroke={2.2} />
            {t('downloads.resumeAll')}
          </BulkBtn>
          <BulkBtn onClick={() => act(() => client.reannounceDownloads())} busy={busy}>
            <IconUsersPlus size={14} stroke={2.2} />
            {t('downloads.askPeers')}
          </BulkBtn>
        </div>
      ) : null}

      <div className="overflow-hidden rounded-2xl border border-white/[0.08] bg-[#121216] shadow-[0_10px_28px_rgba(0,0,0,.3)]">
        <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-4 border-b border-white/[0.06] bg-[#15151A] px-5 py-3 md:grid-cols-[minmax(0,1fr)_190px_120px_110px_84px]">
          <Head>{t('downloads.colRelease')}</Head>
          <Head className="max-md:hidden">{t('downloads.colProgress')}</Head>
          <Head className="max-md:hidden">{t('downloads.colSpeed')}</Head>
          <Head className="max-md:hidden">{t('downloads.colStatus')}</Head>
          <span />
        </div>
        {downloads.map((dl) => (
          <DownloadRowView
            key={dl.id}
            dl={dl}
            live={live[dl.id]}
            busy={busy}
            onPause={() => act(() => client.pauseDownload(dl.id))}
            onResume={() => act(() => client.resumeDownload(dl.id))}
            onRetry={() => act(() => client.retryDownload(dl.id))}
            onAskPeers={() => act(() => client.reannounceDownload(dl.id))}
            onRemove={() => {
              setWipeData(true);
              setConfirm(dl);
            }}
          />
        ))}
        {data === null ? <TableSkeleton rows={6} /> : null}
        {data && downloads.length === 0 ? (
          <div className="px-5 py-14 text-center text-[14px] font-medium text-white/45">
            <IconDownload size={26} stroke={1.6} className="mx-auto mb-2.5 text-white/30" />
            {t('downloads.empty')}
          </div>
        ) : null}
      </div>

      {canSettings ? <DownloadClientsSection /> : null}

      {confirm ? (
        <Modal title={t('downloads.removeTitle')} onClose={() => setConfirm(null)}>
          <p className="text-[13.5px] leading-relaxed text-white/70">
            {t('downloads.removeBody', { title: confirm.title })}
          </p>
          <label className="mt-4 flex cursor-pointer items-center gap-2.5 text-[13.5px] font-semibold text-white/80">
            <input
              type="checkbox"
              checked={wipeData}
              onChange={(e) => setWipeData(e.target.checked)}
              className="h-4 w-4 accent-[#E8536A]"
            />
            {t('downloads.removeData')}
          </label>
          <ModalActions
            onCancel={() => setConfirm(null)}
            cancelLabel={t('common.cancel')}
            onConfirm={() => {
              const dl = confirm;
              setConfirm(null);
              act(() => client.removeDownload(dl.id, { deleteData: wipeData }));
            }}
            confirmLabel={t('downloads.removeConfirm')}
            busy={busy}
          />
        </Modal>
      ) : null}

      {manual ? <ManualGrabModal onClose={() => setManual(false)} onAdded={reload} /> : null}
    </div>
  );
}

function Head({
  children,
  className = '',
}: Readonly<{ children: ReactNode; className?: string }>) {
  return (
    <span className={`text-[9.5px] font-bold uppercase tracking-[.12em] text-white/40 ${className}`}>
      {children}
    </span>
  );
}

function BulkBtn({
  onClick,
  busy,
  children,
}: Readonly<{ onClick: () => void; busy: boolean; children: ReactNode }>) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={busy}
      className="inline-flex items-center gap-1.5 rounded-lg border border-white/[0.10] bg-white/[0.04] px-3 py-2 text-[12.5px] font-semibold text-white/75 transition-colors hover:bg-white/[0.08] hover:text-white disabled:cursor-not-allowed disabled:opacity-50"
    >
      {children}
    </button>
  );
}
