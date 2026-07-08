import { episodeTag, type MediaItem } from '@luma/core';
import { useT } from '@luma/ui';
import { useState } from 'react';
import { lumaClient } from '#web/shared/lib/api';

/** Depleting circular progress ring with the remaining seconds in the centre,
 * shown on the autoplay button (Netflix/Disney+ countdown). */
function CountdownRing({ seconds, total }: Readonly<{ seconds: number; total: number }>) {
  const r = 9;
  const circ = 2 * Math.PI * r;
  const frac = total > 0 ? Math.min(1, Math.max(0, seconds / total)) : 0;
  return (
    <span className="relative inline-flex h-6 w-6 items-center justify-center">
      <svg className="absolute inset-0 h-6 w-6 -rotate-90" viewBox="0 0 24 24" aria-hidden>
        <circle
          cx="12"
          cy="12"
          r={r}
          fill="none"
          stroke="currentColor"
          strokeOpacity="0.25"
          strokeWidth="2.5"
        />
        <circle
          cx="12"
          cy="12"
          r={r}
          fill="none"
          stroke="currentColor"
          strokeWidth="2.5"
          strokeLinecap="round"
          strokeDasharray={circ}
          strokeDashoffset={circ * (1 - frac)}
          style={{ transition: 'stroke-dashoffset 1s linear' }}
        />
      </svg>
      <span className="text-[10px] font-bold leading-none tabular-nums">{seconds}</span>
    </span>
  );
}

/** Netflix-style "up next" card shown near the end of an episode: the next
 * episode's still + title and a countdown, with "Lecture" (play now) and
 * "Annuler" (dismiss). The countdown + autoplay are driven by the Player. */
export function UpNextOverlay({
  next,
  seconds,
  total,
  onPlayNow,
  onCancel,
}: Readonly<{
  next: MediaItem;
  seconds: number;
  total: number;
  onPlayNow: () => void;
  onCancel: () => void;
}>) {
  const t = useT();
  const c = lumaClient();
  const [imgOk, setImgOk] = useState(true);
  const thumb = c.backdropFor(next) ?? c.posterFor(next);
  const tag = episodeTag(next);
  const title = next.episodeTitle ?? next.title;

  return (
    <div className="absolute bottom-28 right-8 z-50 w-90 rounded-2xl border border-white/10 bg-[rgba(18,18,22,.92)] p-4 shadow-pop backdrop-blur-xl">
      <div className="mb-2.5 text-[12px] font-bold uppercase tracking-[.18em] text-accent">
        {t('content.upNext')}
      </div>
      <div className="flex gap-3.5">
        <div className="relative aspect-video w-32 shrink-0 overflow-hidden rounded-md bg-surface-1">
          {thumb && imgOk ? (
            <img
              src={thumb}
              alt=""
              onError={() => setImgOk(false)}
              className="absolute inset-0 h-full w-full object-cover"
            />
          ) : null}
        </div>
        <div className="min-w-0">
          {tag ? <div className="text-[12px] font-semibold text-white/55">{tag}</div> : null}
          <div className="line-clamp-2 font-display text-[16px] font-bold text-white">{title}</div>
          <div className="mt-1 text-[13px] text-white/60">
            {t('player.playingNextIn', { seconds })}
          </div>
        </div>
      </div>
      <div className="mt-3.5 flex gap-2.5">
        <button
          type="button"
          onClick={onPlayNow}
          className="flex flex-1 items-center justify-center gap-2 rounded-md bg-accent px-3 py-2 text-[14px] font-bold text-black transition-colors hover:bg-accent/90"
        >
          <CountdownRing seconds={seconds} total={total} />
          {t('content.play')}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md border border-border-strong bg-white/10 px-3 py-2 text-[14px] font-semibold text-text transition-colors hover:bg-white/15"
        >
          {t('common.cancel')}
        </button>
      </div>
    </div>
  );
}
