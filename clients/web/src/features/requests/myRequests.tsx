// "Mes demandes": a user's own requests with live status/progress, and a
// cancel action for still-pending ones. Slow poll + a page-scoped event
// stream (request.updated reloads, download.progress patches the bar).

import { LumaEvents, type MediaRequest, posterColors, sizedImageUrl } from '@luma/core';
import { useT } from '@luma/ui';
import { IconInbox, IconLoader2, IconX } from '@tabler/icons-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from '@tanstack/react-router';
import { RequestStatusChip } from '#web/features/requests/RequestStatusChip';
import { seasonsSummary } from '#web/features/requests/status';
import { apiBase } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';

export function MyRequestsPage() {
  const t = useT();
  const { client } = useAuth();
  const navigate = useNavigate();
  const [requests, setRequests] = useState<MediaRequest[] | null>(null);
  const [progress, setProgress] = useState<Record<string, number>>({});
  const [busyId, setBusyId] = useState<string | null>(null);

  const reload = useCallback(() => {
    client
      .listRequests({ mine: true })
      .then((v) => setRequests(v.requests))
      .catch(() => undefined);
  }, [client]);

  useEffect(() => {
    reload();
    const iv = setInterval(reload, 15000);
    return () => clearInterval(iv);
  }, [reload]);

  const lastReloadRef = useRef(0);
  useEffect(() => {
    const ev = new LumaEvents(apiBase(), {
      onEvent: (e) => {
        if (e.type === 'request.updated') {
          const now = Date.now();
          if (now - lastReloadRef.current > 1200) {
            lastReloadRef.current = now;
            reload();
          }
        } else if (e.type === 'download.progress' && e.requestId) {
          setProgress((p) => ({ ...p, [e.requestId as string]: e.progress }));
        }
      },
    });
    ev.connect();
    return () => ev.close();
  }, [reload]);

  const cancel = (req: MediaRequest) => {
    setBusyId(req.id);
    client
      .deleteRequest(req.id)
      .then(reload)
      .catch(() => undefined)
      .finally(() => setBusyId(null));
  };

  return (
    <main className="min-w-0 px-10 pb-20 pt-9">
      <h1 className="font-display text-[32px] font-bold leading-tight tracking-[-.02em]">
        {t('requests.myTitle')}
      </h1>
      <p className="mt-1.5 text-[14.5px] font-medium text-dim">{t('requests.mySubtitle')}</p>

      {requests && requests.length === 0 ? (
        <div className="mt-16 flex flex-col items-center text-center">
          <IconInbox size={30} stroke={1.5} className="mb-3 text-dim" />
          <div className="text-[15px] font-semibold">{t('requests.myEmpty')}</div>
          <button
            type="button"
            onClick={() => navigate({ to: '/search' })}
            className="mt-4 rounded-xl bg-accent px-5 py-2.5 text-[14px] font-bold text-accent-ink hover:bg-accent-hover"
          >
            {t('requests.myEmptyCta')}
          </button>
        </div>
      ) : null}

      <div className="mt-6 flex flex-col gap-2.5">
        {(requests ?? []).map((req) => (
          <RequestRow
            key={req.id}
            req={req}
            progress={progress[req.id]}
            busy={busyId === req.id}
            onCancel={() => cancel(req)}
            onOpen={() => {
              if (req.status === 'available') {
                // Best-effort: search takes them to the fiche once scanned.
                navigate({ to: '/search' });
              } else {
                navigate({
                  to: '/discover/$type/$tmdbId',
                  params: { type: req.kind === 'show' ? 'tv' : 'movie', tmdbId: String(req.tmdbId) },
                });
              }
            }}
          />
        ))}
      </div>
    </main>
  );
}

function RequestRow({
  req,
  progress,
  busy,
  onCancel,
  onOpen,
}: Readonly<{
  req: MediaRequest;
  progress?: number;
  busy: boolean;
  onCancel: () => void;
  onOpen: () => void;
}>) {
  const t = useT();
  const [c1, c2] = posterColors(String(req.tmdbId));
  const poster = sizedImageUrl(req.posterUrl, 92);
  const seasons = seasonsSummary(req.seasons);

  return (
    <div className="flex items-center gap-4 rounded-2xl border border-border bg-surface-1 p-3.5">
      <button
        type="button"
        onClick={onOpen}
        className="flex min-w-0 flex-1 items-center gap-4 text-left"
      >
        <div
          className="h-[68px] w-[46px] flex-[0_0_46px] overflow-hidden rounded-lg"
          style={{ background: `linear-gradient(158deg, ${c1}, ${c2})` }}
        >
          {poster ? <img src={poster} alt="" className="h-full w-full object-cover" /> : null}
        </div>
        <div className="min-w-0">
          <div className="truncate text-[15px] font-bold">{req.title}</div>
          <div className="mt-0.5 text-[12.5px] font-medium text-dim">
            {[req.year ? String(req.year) : '', req.kind === 'show' ? (seasons ?? t('requests.allSeasons')) : '']
              .filter(Boolean)
              .join(' · ')}
          </div>
          {req.note ? <div className="mt-1 text-[12px] font-semibold text-[#EF8091]">{req.note}</div> : null}
        </div>
      </button>
      <RequestStatusChip status={req.status} progress={progress ?? req.progress ?? null} />
      {req.status === 'pending' ? (
        <button
          type="button"
          disabled={busy}
          onClick={onCancel}
          title={t('requests.cancel')}
          className="flex h-9 w-9 items-center justify-center rounded-lg border border-white/12 bg-[#1A1A20] text-white/55 hover:text-[#E8536A] disabled:opacity-50"
        >
          {busy ? <IconLoader2 size={15} stroke={2.4} className="animate-spin" /> : <IconX size={15} stroke={2.2} />}
        </button>
      ) : null}
    </div>
  );
}
