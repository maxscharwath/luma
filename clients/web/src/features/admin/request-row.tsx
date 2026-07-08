// One request row in the admin queue: poster, title + type pill + seasons,
// requester, date, status chip, and quick approve/deny on pending rows.

import type { MediaRequest, MessageKey } from '@luma/core';
import { useT } from '@luma/ui';
import { IconCheck, IconX } from '@tabler/icons-react';
import { useState } from 'react';
import { kindMeta, posterGrad } from '#web/features/admin/pipeline-meta';
import { Avatar } from '#web/features/admin/ui';
import { RequestStatusChip } from '#web/features/requests/request-status-chip';
import { seasonsSummary } from '#web/features/requests/status';

function Poster({ req }: Readonly<{ req: MediaRequest }>) {
  const [broken, setBroken] = useState(false);
  return (
    <div
      style={{ background: posterGrad(req.title) }}
      className="relative h-[46px] w-8 flex-[0_0_32px] overflow-hidden rounded-[6px] shadow-[0_5px_14px_rgba(0,0,0,.45)]"
    >
      {req.posterUrl && !broken ? (
        <img
          src={req.posterUrl}
          alt=""
          loading="lazy"
          onError={() => setBroken(true)}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : null}
    </div>
  );
}

export function RequestRowView({
  req,
  canReview,
  onOpen,
  onApprove,
  onDeny,
}: Readonly<{
  req: MediaRequest;
  canReview: boolean;
  onOpen: () => void;
  onApprove: () => void;
  onDeny: () => void;
}>) {
  const t = useT();
  const km = kindMeta(req.kind === 'show' ? 'series' : 'film');
  const seasons = seasonsSummary(req.seasons);
  const sub = [
    req.year ? String(req.year) : '',
    req.kind === 'show' ? (seasons ?? t('requests.allSeasons')) : '',
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <button
      type="button"
      onClick={onOpen}
      className="grid w-full cursor-pointer grid-cols-[minmax(0,1fr)_190px_110px_132px_76px] items-center gap-4 border-b border-white/[0.04] px-5 py-3 text-left transition-colors hover:bg-white/[0.028]"
    >
      <div className="flex min-w-0 items-center gap-3.5">
        <Poster req={req} />
        <div className="min-w-0">
          <div className="flex items-center gap-2.5">
            <span className="truncate text-[14.5px] font-bold">{req.title}</span>
            <span
              className="flex-[0_0_auto] rounded-full px-[7px] py-0.5 text-[8px] font-bold uppercase tracking-[.08em]"
              style={{ color: km.color, background: km.bg }}
            >
              {t(`pipeline.type.${km.typeKey}` as MessageKey)}
            </span>
          </div>
          <div className="mt-[3px] truncate text-[12px] font-medium text-white/50">{sub}</div>
        </div>
      </div>

      <div className="flex min-w-0 items-center gap-2.5">
        <Avatar name={req.requestedByName ?? '?'} size={26} />
        <span className="truncate text-[13px] font-semibold text-white/75">
          {req.requestedByName ?? t('requests.unknownUser')}
        </span>
      </div>

      <span className="text-[12.5px] font-semibold tabular-nums text-white/55">
        {new Date(req.createdAt).toLocaleDateString()}
      </span>

      <div>
        <RequestStatusChip status={req.status} progress={req.progress} />
      </div>

      <div className="flex justify-end gap-1.5">
        {canReview && req.status === 'pending' ? (
          <>
            <span
              role="button"
              tabIndex={-1}
              onClick={(e) => {
                e.stopPropagation();
                onApprove();
              }}
              title={t('requests.approve')}
              className="flex h-8 w-8 items-center justify-center rounded-lg border border-[#46D08D]/30 bg-[#46D08D]/10 text-[#46D08D] transition-colors hover:bg-[#46D08D]/20"
            >
              <IconCheck size={14} stroke={2.6} />
            </span>
            <span
              role="button"
              tabIndex={-1}
              onClick={(e) => {
                e.stopPropagation();
                onDeny();
              }}
              title={t('requests.deny')}
              className="flex h-8 w-8 items-center justify-center rounded-lg border border-[#E8536A]/30 bg-[#E8536A]/10 text-[#E8536A] transition-colors hover:bg-[#E8536A]/20"
            >
              <IconX size={14} stroke={2.6} />
            </span>
          </>
        ) : null}
      </div>
    </button>
  );
}
