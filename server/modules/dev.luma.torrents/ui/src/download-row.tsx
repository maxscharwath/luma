// One row of the downloads queue: release name + target pill, live progress
// bar (WS-fed), speed, seeders-side stats, client pill, status, and a kebab
// (⋮) menu with the row's state-dependent actions (pause/resume, ask more
// peers, retry, tracker link, remove).

import type { DownloadView, MessageKey } from '@luma/module-sdk';
import { useT } from '@luma/module-sdk';
import * as DropdownMenu from '@radix-ui/react-dropdown-menu';
import {
  IconDotsVertical,
  IconExternalLink,
  IconInfoCircle,
  IconMovie,
  IconPlayerPause,
  IconPlayerPlay,
  IconRefresh,
  IconTrash,
  IconUsers,
  IconUsersPlus,
} from '@tabler/icons-react';
import { useNavigate } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { formatBytes, ProgressBar } from '@luma/module-sdk';

/** Live per-download overlay fed by `download.progress` WS frames. */
export interface LiveDl {
  progress: number;
  downBps: number;
  upBps: number;
  peers: number;
  peersSeen: number;
  state: string;
}

const STATUS_COLOR: Record<string, string> = {
  queued: 'rgba(244,243,240,.55)',
  downloading: '#F4B642',
  seeding: '#46D08D',
  completed: '#46D08D',
  imported: '#46D08D',
  paused: 'rgba(244,243,240,.55)',
  failed: '#E8536A',
  removed: 'rgba(244,243,240,.4)',
};

const MENU =
  'z-50 min-w-[184px] rounded-xl border border-white/[0.10] bg-[#16161C] p-1.5 shadow-[0_12px_32px_rgba(0,0,0,.45)]';

/** The season/episode pill for the release title (movies get none). */
function targetPill(dl: DownloadView): string | null {
  const s = String(dl.season ?? 0).padStart(2, '0');
  if (dl.kind === 'season') return `S${s}`;
  if (dl.kind === 'episode') return `S${s}E${String(dl.episodes?.[0] ?? 0).padStart(2, '0')}`;
  return null;
}

export function DownloadRowView({
  dl,
  live,
  busy,
  onPause,
  onResume,
  onRetry,
  onAskPeers,
  onRemove,
}: Readonly<{
  dl: DownloadView;
  live?: LiveDl;
  busy: boolean;
  onPause: () => void;
  onResume: () => void;
  onRetry: () => void;
  onAskPeers: () => void;
  onRemove: () => void;
}>) {
  const t = useT();
  const navigate = useNavigate();
  const status = live?.state && dl.status !== 'imported' ? live.state : dl.status;
  const progress = live?.progress ?? dl.progress;
  const color = STATUS_COLOR[status] ?? 'rgba(244,243,240,.55)';
  const active = status === 'downloading' || status === 'queued';
  const pausable = active;
  const resumable = status === 'paused';
  // "Ask more peers" only makes sense while the torrent is live in the engine.
  const canAskPeers = active || status === 'seeding';
  // Retry is offered in every state: the backend does the right thing per status
  // (completed/imported -> re-import; anything else -> reset + re-add). Useful to
  // force-restart a stuck download or re-run an import, not just failed grabs.
  const retryable = true;

  const targetLabel = targetPill(dl);

  // "Open in LUMA" jumps to the library fiche, once the title has been imported.
  const localId = dl.localId;
  const openInLuma = localId
    ? () =>
        navigate({
          to: dl.kind === 'movie' ? '/movie/$id' : '/show/$id',
          params: { id: localId },
        })
    : null;

  return (
    <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 border-b border-white/[0.04] px-5 py-3 md:grid-cols-[minmax(0,1fr)_190px_120px_110px_84px]">
      <div className="flex min-w-0 items-center gap-3">
        {dl.posterUrl ? (
          <img
            src={dl.posterUrl}
            alt=""
            loading="lazy"
            className="h-11 w-[30px] flex-[0_0_auto] rounded-[3px] bg-white/5 object-cover"
          />
        ) : (
          <div className="flex h-11 w-[30px] flex-[0_0_auto] items-center justify-center rounded-[3px] bg-white/[0.05]">
            <IconMovie size={13} className="text-white/25" />
          </div>
        )}
        <div className="min-w-0">
          <div className="flex items-center gap-2.5">
            <span className="truncate text-[13.5px] font-bold" title={dl.releaseTitle}>
              {dl.title}
            </span>
            {targetLabel ? (
              <span className="flex-[0_0_auto] rounded-full bg-[#86A8FF]/[0.14] px-[7px] py-0.5 text-[9px] font-bold text-[#86A8FF]">
                {targetLabel}
              </span>
            ) : null}
          </div>
          <div className="mt-[3px] flex items-center gap-1.5 text-[11.5px] font-medium text-white/40">
            <span className="truncate" title={dl.releaseTitle}>
              {dl.releaseTitle}
            </span>
            {dl.indexerName ? (
              <span className="flex-[0_0_auto] text-white/30">· {dl.indexerName}</span>
            ) : null}
            {dl.detailsUrl ? (
              <a
                href={dl.detailsUrl}
                target="_blank"
                rel="noreferrer"
                title={t('downloads.viewOnTracker')}
                className="flex-[0_0_auto] text-white/40 hover:text-accent"
              >
                <IconExternalLink size={12} stroke={2} />
              </a>
            ) : null}
          </div>
          {dl.error ? (
            <div className="mt-1 truncate text-[11.5px] font-semibold text-[#EF8091]">
              {dl.error}
            </div>
          ) : null}
        </div>
      </div>

      <div className="max-md:hidden">
        <ProgressBar pct={progress * 100} color={color} height={5} />
        <div className="mt-1 flex items-center justify-between text-[11px] font-semibold tabular-nums text-white/45">
          <span>{Math.round(progress * 100)}%</span>
          {dl.sizeBytes != null ? <span>{formatBytes(dl.sizeBytes)}</span> : null}
        </div>
      </div>

      <div className="text-[11.5px] font-semibold tabular-nums text-white/55 max-md:hidden">
        {live && active ? (
          <>
            <div className="text-[#46D08D]">{formatBytes(live.downBps)}/s</div>
            <div className="flex items-center gap-1.5 text-white/35">
              <span>{formatBytes(live.upBps)}/s</span>
              <span
                className={`flex items-center gap-0.5 ${live.peers > 0 ? 'text-[#86A8FF]' : 'text-[#F4B642]'}`}
                title={t('downloads.peersDetail', {
                  live: String(live.peers),
                  seen: String(live.peersSeen),
                })}
              >
                <IconUsers size={11} stroke={2} />
                {live.peersSeen > live.peers ? `${live.peers}/${live.peersSeen}` : live.peers}
              </span>
            </div>
          </>
        ) : (
          <span className="text-white/30">-</span>
        )}
      </div>

      <div className="max-md:hidden">
        <span
          className="inline-flex items-center gap-1.5 rounded-full px-[10px] py-[4px] text-[11px] font-bold"
          style={{ color, background: `${STATUS_COLOR[status] ?? '#fff'}22` }}
        >
          <span
            className={`h-1.5 w-1.5 rounded-full ${active ? 'animate-pulse' : ''}`}
            style={{ background: color }}
          />
          {t(`downloads.st.${status}` as MessageKey)}
        </span>
        <div className="mt-1 text-[10.5px] font-medium text-white/35">{dl.clientName}</div>
      </div>

      <div className="flex justify-end">
        <DropdownMenu.Root>
          <DropdownMenu.Trigger asChild>
            <button
              type="button"
              aria-label={t('downloads.rowActions')}
              className="flex h-8 w-8 items-center justify-center rounded-lg border border-white/12 bg-[#1A1A20] text-white/70 outline-none transition-colors hover:text-white data-[state=open]:bg-white/[0.08] data-[state=open]:text-white"
            >
              <IconDotsVertical size={15} stroke={2} />
            </button>
          </DropdownMenu.Trigger>
          <DropdownMenu.Portal>
            <DropdownMenu.Content align="end" sideOffset={6} className={MENU}>
              {pausable ? (
                <RowMenuItem
                  icon={<IconPlayerPause size={14} stroke={2.2} />}
                  label={t('downloads.pause')}
                  onSelect={onPause}
                  disabled={busy}
                />
              ) : null}
              {resumable ? (
                <RowMenuItem
                  icon={<IconPlayerPlay size={14} stroke={2.2} />}
                  label={t('downloads.resume')}
                  onSelect={onResume}
                  disabled={busy}
                />
              ) : null}
              {canAskPeers ? (
                <RowMenuItem
                  icon={<IconUsersPlus size={14} stroke={2.2} />}
                  label={t('downloads.askPeers')}
                  onSelect={onAskPeers}
                  disabled={busy}
                />
              ) : null}
              {retryable ? (
                <RowMenuItem
                  icon={<IconRefresh size={14} stroke={2.2} />}
                  label={t('downloads.retry')}
                  onSelect={onRetry}
                  disabled={busy}
                />
              ) : null}
              {openInLuma ? (
                <RowMenuItem
                  icon={<IconInfoCircle size={14} stroke={2} />}
                  label={t('downloads.openInLuma')}
                  onSelect={openInLuma}
                />
              ) : null}
              {dl.detailsUrl ? (
                <RowMenuItem
                  icon={<IconExternalLink size={14} stroke={2} />}
                  label={t('downloads.viewOnTracker')}
                  onSelect={() => {
                    if (dl.detailsUrl) window.open(dl.detailsUrl, '_blank', 'noopener,noreferrer');
                  }}
                />
              ) : null}
              <DropdownMenu.Separator className="my-1 h-px bg-white/[0.07]" />
              <RowMenuItem
                icon={<IconTrash size={14} stroke={2} />}
                label={t('downloads.remove')}
                onSelect={onRemove}
                disabled={busy}
                danger
              />
            </DropdownMenu.Content>
          </DropdownMenu.Portal>
        </DropdownMenu.Root>
      </div>
    </div>
  );
}

function RowMenuItem({
  icon,
  label,
  onSelect,
  disabled,
  danger,
}: Readonly<{
  icon: ReactNode;
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  danger?: boolean;
}>) {
  return (
    <DropdownMenu.Item
      disabled={disabled}
      onSelect={onSelect}
      className={`flex cursor-pointer items-center gap-2.5 rounded-lg px-2.5 py-2 text-[13px] font-semibold outline-none transition-colors data-[disabled]:cursor-not-allowed data-[disabled]:opacity-40 ${danger ? 'text-[#EF8091] data-[highlighted]:bg-[#E8536A]/[0.14]' : 'text-white/80 data-[highlighted]:bg-white/[0.07] data-[highlighted]:text-white'}`}
    >
      {icon}
      {label}
    </DropdownMenu.Item>
  );
}
