// Per-element "Traitements" panel (admin-only): shows, for one film/episode/show,
// the status of every processing treatment applied to it (probe, TMDB, storyboard
// previews, markers, embedding), plus a one-click reprocess. Plex-style: see at a
// glance what has and hasn't been done to this exact element.

import { hasPermission, type MessageKey, type Treatment } from '@luma/core';
import { useT } from '@luma/ui';
import {
  type Icon as TablerIcon,
  IconAlertTriangleFilled,
  IconCircle,
  IconCircleCheckFilled,
  IconLoader2,
  IconRefresh,
} from '@tabler/icons-react';
import { useCallback, useEffect, useState } from 'react';
import { useAuth } from '#web/shared/lib/auth';

type Kind = 'item' | 'show';

const STATUS = {
  done: { icon: IconCircleCheckFilled, cls: 'text-[#46D08D]' },
  running: { icon: IconLoader2, cls: 'text-[#F4B642]', spin: true },
  pending: { icon: IconLoader2, cls: 'text-[#F4B642]/70' },
  failed: { icon: IconAlertTriangleFilled, cls: 'text-[#E8536A]' },
  missing: { icon: IconCircle, cls: 'text-white/25' },
} satisfies Record<string, { icon: TablerIcon; cls: string; spin?: boolean }>;

export function TreatmentsPanel({ kind, id }: Readonly<{ kind: Kind; id: string }>) {
  const t = useT();
  const { user, client } = useAuth();
  const [treatments, setTreatments] = useState<Treatment[] | null>(null);
  const [busy, setBusy] = useState(false);
  const admin = !!user && hasPermission(user, 'settings.manage');

  const load = useCallback(() => {
    (kind === 'show' ? client.showProcessing(id) : client.itemProcessing(id))
      .then((r) => setTreatments(r.treatments))
      .catch(() => setTreatments(null));
  }, [client, kind, id]);

  useEffect(() => {
    if (admin) load();
  }, [admin, load]);

  // Refresh while anything is still processing.
  useEffect(() => {
    if (!admin || !treatments?.some((x) => x.status === 'running' || x.status === 'pending')) return;
    const iv = setInterval(load, 3000);
    return () => clearInterval(iv);
  }, [admin, treatments, load]);

  if (!admin || !treatments) return null;

  const reprocess = () => {
    setBusy(true);
    client
      .reprocessSubject(kind, id)
      .then(() => setTimeout(load, 1500))
      .catch(() => {})
      .finally(() => setTimeout(() => setBusy(false), 1500));
  };

  return (
    <section className="mt-8 px-6 md:px-16">
      <div className="flex flex-wrap items-center gap-x-6 gap-y-2.5 rounded-xl border border-white/8 bg-white/[0.03] px-5 py-4">
        <span className="text-[11px] font-bold uppercase tracking-widest text-white/45">
          {t('pipeline.treatments')}
        </span>
        {treatments.map((tr) => {
          const meta = STATUS[tr.status as keyof typeof STATUS] ?? STATUS.missing;
          const Icon = meta.icon;
          const spin = 'spin' in meta && meta.spin;
          return (
            <span
              key={tr.key}
              className="inline-flex items-center gap-1.5 text-[13.5px]"
              title={t(`pipeline.st.${tr.status}` as MessageKey)}
            >
              <Icon size={16} className={`${meta.cls} ${spin ? 'animate-spin' : ''}`} />
              <span className="text-white/80">{t(`pipeline.t.${tr.key}` as MessageKey)}</span>
            </span>
          );
        })}
        <button
          type="button"
          onClick={reprocess}
          disabled={busy}
          className="ml-auto inline-flex items-center gap-1.5 rounded-md border border-white/12 bg-white/8 px-3.5 py-2 text-[13px] font-semibold text-white/85 transition-colors hover:bg-white/12 disabled:opacity-50"
        >
          <IconRefresh size={15} stroke={2} className={busy ? 'animate-spin' : ''} />
          {t('pipeline.reprocessBtn')}
        </button>
      </div>
    </section>
  );
}
