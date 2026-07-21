import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n';
import { IconClose } from './icons';
import type { PlayerController, PlayerMeter, PlayerStats } from './types';

/** How many samples of history each sparkline keeps (~40s at the 500ms poll). */
const HISTORY = 80;
const SPARK_W = 148;
const SPARK_H = 30;

/** A tiny auto-scaled line chart for one live series. Pure SVG (no canvas / lib)
 * so it renders on legacy TV webviews too. The window is scaled to its own
 * min/max with a hair of headroom, and a faint area fill sits under the line. */
function Sparkline({ data, color }: Readonly<{ data: number[]; color: string }>) {
  if (data.length < 2) return <svg width={SPARK_W} height={SPARK_H} aria-hidden="true" />;
  let min = Math.min(...data);
  let max = Math.max(...data);
  if (max === min) {
    max += 1;
    min -= 1;
  }
  const span = max - min;
  const step = SPARK_W / (data.length - 1);
  const y = (v: number) => SPARK_H - 2 - ((v - min) / span) * (SPARK_H - 4);
  const line = data.map((v, i) => `${(i * step).toFixed(1)},${y(v).toFixed(1)}`).join(' ');
  const area = `0,${SPARK_H} ${line} ${SPARK_W},${SPARK_H}`;
  return (
    <svg width={SPARK_W} height={SPARK_H} aria-hidden="true" style={{ display: 'block' }}>
      <polyline points={area} fill={color} fillOpacity={0.12} stroke="none" />
      <polyline
        points={line}
        fill="none"
        stroke={color}
        strokeWidth={1.5}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}

/** One labelled meter row: label + current value on top, its sparkline below. */
function MeterRow({ meter, data }: Readonly<{ meter: PlayerMeter; data: number[] }>) {
  const color = meter.color ?? 'rgba(244,243,240,0.7)';
  return (
    <div className="flex flex-col gap-1">
      <div className="flex justify-between gap-6 font-sans text-[13px] font-medium">
        <span className="text-[rgba(244,243,240,0.5)]">{meter.label}</span>
        <span className="whitespace-nowrap text-right tabular-nums text-[rgba(244,243,240,0.82)]">
          {meter.display}
        </span>
      </div>
      <Sparkline data={data} color={color} />
    </div>
  );
}

/**
 * Discreet top-left "stats for nerds" overlay (§9). Polls the controller's
 * `getStats()` snapshot twice a second and renders each populated field as a
 * dim-label / tabular-value row, matching the design's frosted card. Live numeric
 * series (`meters`) are accumulated into a rolling per-key history and drawn as
 * sparklines. Read-only: it never drives the engine, so it carries no D-pad focus
 * beyond the close X.
 */
export function StatsPanel({
  controller,
  onClose,
}: Readonly<{ controller: PlayerController; onClose: () => void }>) {
  const t = useT();
  const [s, setS] = useState<PlayerStats>(() => controller.getStats());
  // Rolling numeric history per meter key, kept across polls in a ref (drawing is
  // driven by the setS re-render, not by mutating this).
  const historyRef = useRef<Map<string, number[]>>(new Map());
  // The controller is rebuilt on every parent render (which happens ~4x/s while
  // playing, via timeupdate). Keep the latest `getStats` in a ref so the poll
  // interval below can be set up ONCE - depending on `controller` identity would
  // tear down and recreate the 500ms timer faster than it ever fires, freezing
  // the panel. `getStats` itself always reads the newest values.
  const getStatsRef = useRef(controller.getStats);
  getStatsRef.current = controller.getStats;

  // Refresh the live counters 2x/s: buffer, dropped frames and bitrate drift
  // slowly enough that a faster tick would only add repaint cost. Each snapshot's
  // numeric `meters` are appended to the per-key rolling history the sparklines
  // draw from (kept in the ref so recording never re-creates the effect).
  useEffect(() => {
    const record = (snap: PlayerStats) => {
      const hist = historyRef.current;
      const live = new Set<string>();
      for (const m of snap.meters ?? []) {
        live.add(m.key);
        const series = hist.get(m.key) ?? [];
        series.push(Number.isFinite(m.value) ? m.value : 0);
        if (series.length > HISTORY) series.shift();
        hist.set(m.key, series);
      }
      // Drop history for series no longer reported (e.g. engine changed).
      for (const key of [...hist.keys()]) if (!live.has(key)) hist.delete(key);
    };
    const tick = () => {
      const snap = getStatsRef.current();
      record(snap);
      setS(snap);
    };
    tick();
    const id = setInterval(tick, 500);
    return () => clearInterval(id);
  }, []);

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

  const meters = s.meters ?? [];

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
      {meters.length > 0 && (
        <div className="mt-3 flex flex-col gap-3 border-t border-[rgba(255,255,255,0.08)] pt-3">
          {meters.map((m) => (
            <MeterRow key={m.key} meter={m} data={historyRef.current.get(m.key) ?? []} />
          ))}
        </div>
      )}
    </div>
  );
}
