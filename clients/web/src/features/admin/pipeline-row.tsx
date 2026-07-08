// One element row in the pipeline table: poster, title + kind badge, a subtitle
// (metadata, or the failing/running stage), the treatment "flow" of status dots,
// the overall status pill, and a reprocess shortcut.

import { type ElementRow, type MessageKey, type Translate, type Treatment } from '@luma/core';
import { useT } from '@luma/ui';
import { IconCheck, IconLoader2, IconRefresh, IconX } from '@tabler/icons-react';
import { useState } from 'react';
import { fmtDur, kindMeta, overallMeta, posterGrad, statusMeta } from '#web/features/admin/pipeline-meta';
import { useAuth } from '#web/shared/lib/auth';

/** The secondary line: on failure/run it names the stage(s); otherwise metadata. */
function subLine(t: Translate, el: ElementRow): { text: string; color: string } {
  const names = (pred: (x: Treatment) => boolean) =>
    el.treatments.filter(pred).map((x) => t(`pipeline.t.${x.key}` as MessageKey)).join(', ');
  if (el.overall === 'failed') {
    return { text: `${t('pipeline.st.failed')} : ${names((x) => x.status === 'failed')}`, color: '#EF8091' };
  }
  if (el.overall === 'running') {
    return { text: `${t('pipeline.st.running')} : ${names((x) => x.status === 'running')}`, color: 'rgba(244,243,240,.6)' };
  }
  const dur = fmtDur(el.durationMs);
  let text: string;
  if (el.kind === 'series') {
    text = [el.genre, el.seasonCount ? `${el.seasonCount} ${t('pipeline.seasons')}` : '']
      .filter(Boolean)
      .join(' · ');
  } else if (el.kind === 'episode') {
    text = [t('pipeline.type.episode'), dur].filter(Boolean).join(' · ');
  } else {
    text = [el.year ? String(el.year) : '', el.genre, dur].filter(Boolean).join(' · ');
  }
  return { text, color: 'rgba(244,243,240,.5)' };
}

function Poster({
  id,
  kind,
  seed,
  poster,
}: Readonly<{ id: string; kind: string; seed: string; poster?: string | null }>) {
  const { client } = useAuth();
  const [broken, setBroken] = useState(false);
  // Prefer the cached TMDB poster; fall back to the by-id endpoint, then the
  // gradient placeholder (onError) if neither has real art.
  const src =
    (poster ? client.resolveArt(poster) : null) ??
    (kind === 'series' ? client.showPosterUrl(id) : client.posterUrl(id));
  return (
    <div
      style={{ background: posterGrad(seed) }}
      className="relative h-[46px] w-8 flex-[0_0_32px] overflow-hidden rounded-[6px] shadow-[0_5px_14px_rgba(0,0,0,.45)]"
    >
      {!broken ? (
        <img
          src={src}
          alt=""
          loading="lazy"
          onError={() => setBroken(true)}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : null}
    </div>
  );
}

function FlowDots({ treatments }: Readonly<{ treatments: Treatment[] }>) {
  const t = useT();
  return (
    <div className="flex items-center">
      {treatments.map((tr, i) => {
        const m = statusMeta(tr.status);
        const prevDone = i > 0 && treatments[i - 1]?.status === 'done';
        return (
          <span key={tr.key} className="flex items-center">
            {i > 0 ? (
              <span
                className="h-0.5 w-3 flex-[0_0_12px] rounded-[2px]"
                style={{ background: prevDone ? '#46D08D' : 'rgba(255,255,255,.12)' }}
              />
            ) : null}
            <span
              title={`${t(`pipeline.t.${tr.key}` as MessageKey)} - ${t(`pipeline.st.${tr.status}` as MessageKey)}`}
              className="flex h-[19px] w-[19px] flex-[0_0_19px] items-center justify-center rounded-full border"
              style={{ background: m.bg, borderColor: m.ring, color: m.color }}
            >
              {tr.status === 'done' ? <IconCheck size={11} stroke={3.2} /> : null}
              {tr.status === 'failed' ? <IconX size={10} stroke={3.4} /> : null}
              {tr.status === 'running' ? <IconLoader2 size={11} stroke={2.8} className="animate-spin" /> : null}
              {tr.status === 'pending' || tr.status === 'missing' ? (
                <span className="h-1.5 w-1.5 rounded-full" style={{ background: 'currentColor' }} />
              ) : null}
            </span>
          </span>
        );
      })}
    </div>
  );
}

export function ElementRowView({
  el,
  onOpen,
  onReprocess,
}: Readonly<{ el: ElementRow; onOpen: () => void; onReprocess: () => void }>) {
  const t = useT();
  const km = kindMeta(el.kind);
  const om = overallMeta(el.overall);
  const sub = subLine(t, el);

  return (
    <button
      type="button"
      onClick={onOpen}
      className="grid w-full cursor-pointer grid-cols-[minmax(0,1fr)_150px_132px_46px] items-center gap-4 border-b border-white/[0.04] px-5 py-3 text-left transition-colors hover:bg-white/[0.028]"
    >
      <div className="flex min-w-0 items-center gap-3.5">
        <Poster id={el.id} kind={el.kind} seed={el.title} poster={el.poster} />
        <div className="min-w-0">
          <div className="flex items-center gap-2.5">
            <span className="truncate text-[14.5px] font-bold">{el.title}</span>
            <span
              className="flex-[0_0_auto] rounded-full px-[7px] py-0.5 text-[8px] font-bold uppercase tracking-[.08em]"
              style={{ color: km.color, background: km.bg }}
            >
              {t(`pipeline.type.${km.typeKey}` as MessageKey)}
            </span>
          </div>
          <div className="mt-[3px] truncate text-[12px] font-medium" style={{ color: sub.color }}>
            {sub.text}
          </div>
        </div>
      </div>

      <FlowDots treatments={el.treatments} />

      <div>
        <span
          className="inline-flex items-center gap-1.5 rounded-full px-[11px] py-[5px] text-[11.5px] font-bold"
          style={{ color: om.color, background: om.bg }}
        >
          <span
            className={`h-1.5 w-1.5 rounded-full ${om.pulse ? 'animate-pulse' : ''}`}
            style={{ background: om.dot }}
          />
          {t(`pipeline.overall.${el.overall}` as MessageKey)}
        </span>
      </div>

      <div className="flex justify-end">
        <span
          role="button"
          tabIndex={-1}
          onClick={(e) => {
            e.stopPropagation();
            onReprocess();
          }}
          title={t('pipeline.reprocessItem')}
          className="flex h-8 w-8 items-center justify-center rounded-lg border border-accent/25 bg-accent/10 text-accent transition-colors hover:bg-accent/20"
        >
          <IconRefresh size={14} stroke={2.3} />
        </span>
      </div>
    </button>
  );
}
