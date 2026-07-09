// Slide-in element drawer: poster + identity, the list of treatments with their
// status (and, on failure, the error + a per-stage retry), a series episode
// aggregate, and the "reprocess this element" action.

import type { ElementRow, MessageKey } from '@luma/core';
import { useT } from '@luma/ui';
import { IconLoader2, IconRefresh, IconX } from '@tabler/icons-react';
import { useState } from 'react';
import { fmtDur, kindMeta, posterGrad, statusMeta } from '#web/features/admin/pipeline-meta';
import { useAuth } from '#web/shared/lib/auth';

function DrawerPoster({ el }: Readonly<{ el: ElementRow }>) {
  const { client } = useAuth();
  const [broken, setBroken] = useState(false);
  const src =
    (el.poster ? client.resolveArt(el.poster) : null) ??
    (el.kind === 'series' ? client.showPosterUrl(el.id) : client.posterUrl(el.id));
  return (
    <div
      className="relative h-[104px] w-[70px] flex-[0_0_70px] overflow-hidden rounded-[10px] shadow-[0_10px_24px_rgba(0,0,0,.5)]"
      style={{ background: posterGrad(el.title) }}
    >
      {!broken ? (
        <img
          src={src}
          alt=""
          onError={() => setBroken(true)}
          className="absolute inset-0 h-full w-full object-cover"
        />
      ) : null}
    </div>
  );
}

function baseSub(el: ElementRow, dur: string, seasons: string): string {
  if (el.kind === 'series') {
    return [el.genre, el.seasonCount ? `${el.seasonCount} ${seasons}` : '']
      .filter(Boolean)
      .join(' · ');
  }
  if (el.kind === 'episode') return dur;
  return [el.year ? String(el.year) : '', el.genre, dur].filter(Boolean).join(' · ');
}

export function PipelineDrawer({
  el,
  busy,
  onClose,
  onReprocess,
  onRetryStage,
}: Readonly<{
  el: ElementRow | null;
  busy: boolean;
  onClose: () => void;
  onReprocess: () => void;
  onRetryStage: (stage: string) => void;
}>) {
  const t = useT();
  const open = !!el;
  const km = kindMeta(el?.kind ?? 'film');
  const dur = fmtDur(el?.durationMs);
  const eps = el?.epStats;

  return (
    <>
      <button
        type="button"
        aria-label={t('common.close')}
        onClick={onClose}
        className={`fixed inset-0 z-[60] bg-[rgba(4,4,6,.6)] backdrop-blur-[2px] transition-opacity ${open ? 'opacity-100' : 'pointer-events-none opacity-0'}`}
      />
      <aside
        className="fixed right-0 top-0 z-[61] flex h-screen w-[460px] max-w-[92vw] flex-col border-l border-white/[0.09] bg-[#0E0E12] shadow-[-20px_0_60px_rgba(0,0,0,.6)] transition-transform duration-300 ease-[cubic-bezier(.22,1,.36,1)]"
        style={{ transform: open ? 'translateX(0)' : 'translateX(105%)' }}
      >
        {el ? (
          <>
            <div className="border-b border-white/[0.07] px-6 py-5">
              <div className="mb-4 flex items-center justify-between">
                <span className="text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
                  {t('pipeline.elementSheet')}
                </span>
                <button type="button" onClick={onClose} className="text-white/60 hover:text-white">
                  <IconX size={20} stroke={2.1} />
                </button>
              </div>
              <div className="flex gap-4">
                <DrawerPoster el={el} />
                <div className="min-w-0 pt-1">
                  <span
                    className="rounded-full px-[9px] py-[3px] text-[9.5px] font-bold uppercase tracking-[.1em]"
                    style={{ color: km.color, background: km.bg }}
                  >
                    {t(`pipeline.type.${km.typeKey}` as MessageKey)}
                  </span>
                  <h2 className="mt-2.5 font-display text-[21px] font-bold leading-[1.12]">
                    {el.title}
                  </h2>
                  <div className="mt-1.5 text-[12.5px] font-medium text-white/50">
                    {baseSub(el, dur, t('pipeline.seasons'))}
                  </div>
                </div>
              </div>
            </div>

            <div className="flex-1 overflow-y-auto px-6 py-5">
              <div className="mb-3 text-[10px] font-bold uppercase tracking-[.14em] text-white/40">
                {t('pipeline.treatments')}
              </div>
              <div className="flex flex-col gap-2.5">
                {el.treatments.map((tr) => {
                  const m = statusMeta(tr.status);
                  const failed = tr.status === 'failed';
                  return (
                    <div
                      key={tr.key}
                      className="rounded-xl border border-white/[0.07] bg-[#121216] px-4 py-3.5"
                    >
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-[14px] font-bold">
                          {t(`pipeline.t.${tr.key}` as MessageKey)}
                        </span>
                        <div className="flex items-center gap-2">
                          <span
                            className="inline-flex items-center gap-1.5 rounded-full px-[11px] py-1 text-[11.5px] font-bold"
                            style={{ color: m.color, background: m.bg }}
                          >
                            <span
                              className={`h-1.5 w-1.5 rounded-full ${m.pulse ? 'animate-pulse' : ''}`}
                              style={{ background: m.dot }}
                            />
                            {t(`pipeline.st.${tr.status}` as MessageKey)}
                          </span>
                          {/* Run just this stage now, at top priority (also acts as a retry on failure). */}
                          <button
                            type="button"
                            onClick={() => onRetryStage(tr.key)}
                            disabled={busy}
                            title={failed ? t('pipeline.retryStage') : t('pipeline.runStage')}
                            className={`flex h-7 w-7 flex-[0_0_28px] items-center justify-center rounded-lg border disabled:opacity-50 ${failed ? 'border-accent/30 bg-accent/10 text-accent' : 'border-white/12 bg-white/[0.06] text-white/65 hover:text-white'}`}
                          >
                            <IconRefresh
                              size={13}
                              stroke={2.3}
                              className={busy ? 'animate-spin' : ''}
                            />
                          </button>
                        </div>
                      </div>
                      {failed && tr.error ? (
                        <div className="mt-2.5 rounded-lg border border-[#E8536A]/[0.18] bg-[#E8536A]/[0.08] px-[11px] py-2.5 text-[12px] leading-[1.4] text-[#EF8091]">
                          {tr.error}
                        </div>
                      ) : null}
                    </div>
                  );
                })}
              </div>

              {el.kind === 'series' && eps ? (
                <div className="mt-5 rounded-xl border border-white/[0.07] bg-[#0F0F13] px-4 py-3.5">
                  <div className="mb-2 text-[10px] font-bold uppercase tracking-[.12em] text-white/40">
                    {t('pipeline.epsAggregated')}
                  </div>
                  <div className="text-[12.5px] font-semibold leading-[1.5] text-white/70">
                    {eps.episodes} {t('pipeline.episodesWord')} · {t('pipeline.t.probe')}{' '}
                    {eps.probed}/{eps.episodes} · {t('pipeline.t.storyboard')} {eps.storyboarded}/
                    {eps.episodes} · {t('pipeline.t.markers')} {eps.markerSeasons}/{eps.seasons}
                  </div>
                  <div className="mt-1.5 text-[11.5px] font-medium text-white/[0.42]">
                    {t('pipeline.epsNote')}
                  </div>
                </div>
              ) : null}
            </div>

            <div className="border-t border-white/[0.07] px-6 py-4.5">
              <button
                type="button"
                onClick={onReprocess}
                disabled={busy}
                className="flex w-full items-center justify-center gap-2.5 rounded-xl bg-accent px-4 py-3.5 text-[14px] font-bold text-[#0A0A0C] transition-colors hover:bg-accent-hover disabled:opacity-60"
              >
                {busy ? (
                  <IconLoader2 size={16} stroke={2.3} className="animate-spin" />
                ) : (
                  <IconRefresh size={16} stroke={2.3} />
                )}
                {t('pipeline.reprocessElement')}
              </button>
              <div className="mt-2.5 text-center text-[11.5px] font-medium text-white/[0.42]">
                {t('pipeline.reprocessNote')}
              </div>
            </div>
          </>
        ) : null}
      </aside>
    </>
  );
}
