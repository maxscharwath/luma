import { useEffect, useState } from 'react';
import { useT } from '../i18n';
import { IconClose } from './icons';
import type { PlayerController, PlayerStats } from './types';

/**
 * Discreet top-left "stats for nerds" overlay (§9). Polls the controller's
 * `getStats()` snapshot twice a second and renders each populated field as a
 * dim-label / tabular-value row, matching the design's frosted card. Read-only:
 * it never drives the engine, so it carries no D-pad focus beyond the close X.
 */
export function StatsPanel({
  controller,
  onClose,
}: Readonly<{ controller: PlayerController; onClose: () => void }>) {
  const t = useT();
  const [s, setS] = useState<PlayerStats>(controller.getStats());

  // Refresh the live counters 2x/s: buffer, dropped frames and bitrate drift
  // slowly enough that a faster tick would only add repaint cost.
  useEffect(() => {
    const id = setInterval(() => setS(controller.getStats()), 500);
    return () => clearInterval(id);
  }, [controller]);

  const rows: [string, string][] = [];
  const push = (label: string, value?: string) => {
    if (value != null && value !== '') rows.push([label, value]);
  };

  push(t('stats.resolution'), s.resolution);
  // fps has no catalog key of its own and is a property of the video, so fold it
  // onto the video-codec row rather than invent a label.
  push(t('stats.video'), [s.videoCodec, s.fps].filter(Boolean).join(' · '));
  push(t('stats.audio'), s.audioFormat);
  push(t('stats.avgBitrate'), s.bitrate);
  push(t('stats.buffer'), s.buffer);
  push(t('stats.droppedFrames'), s.dropped);
  push(t('stats.playback'), s.mode);
  // Controller-supplied extra rows already carry their own localized labels.
  for (const e of s.extra ?? []) push(e.label, e.value);

  return (
    <div className="absolute top-[100px] left-[34px] z-20 min-w-[280px] rounded-[14px] border border-[rgba(255,255,255,0.1)] bg-[rgba(8,8,11,0.74)] px-[22px] py-[18px] backdrop-blur-lg">
      <div className="mb-3 flex items-center justify-between gap-6">
        <span className="font-sans text-[11px] font-bold uppercase tracking-[0.16em] text-[rgba(244,243,240,0.5)]">
          {t('stats.title')}
        </span>
        <button
          type="button"
          onClick={onClose}
          aria-label={t('common.close')}
          className="flex flex-none cursor-pointer items-center justify-center rounded-full border-none bg-[rgba(255,255,255,0.08)] p-1 text-[rgba(244,243,240,0.5)] outline-none"
        >
          <IconClose size={15} />
        </button>
      </div>
      <div className="flex flex-col gap-2">
        {rows.map(([k, val]) => (
          <div key={k} className="flex justify-between gap-6 font-sans text-[13px] font-medium">
            <span className="text-[rgba(244,243,240,0.5)]">{k}</span>
            <span className="whitespace-nowrap text-right tabular-nums text-[rgba(244,243,240,0.82)]">
              {val}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}
