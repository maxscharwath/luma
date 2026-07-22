import { useEffect, useRef, useState } from 'react';
import { Pressable } from 'react-native';
import { useT } from '../i18n';
import { Polyline, Svg } from '../primitives/svg';
import { Txt } from '../primitives/Text';
import { Box } from '../system/Box';
import { fonts } from '../tokens';
import { IconClose } from './icons';
import type { PlayerController, PlayerMeter, PlayerStats } from './types';

/** How many samples of history each sparkline keeps (~40s at the 500ms poll). */
const HISTORY = 80;
const SPARK_W = 148;
const SPARK_H = 30;

/** A tiny auto-scaled line chart for one live series. Plain SVG (no canvas, no
 * charting library) so it renders on a legacy TV webview and on a native TV
 * alike. The window is scaled to its own min/max with a hair of headroom, and a
 * faint area fill sits under the line. */
function Sparkline({ data, color }: Readonly<{ data: number[]; color: string }>) {
  if (data.length < 2) return <Svg width={SPARK_W} height={SPARK_H} />;
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
    <Svg width={SPARK_W} height={SPARK_H}>
      <Polyline points={area} fill={color} fillOpacity={0.12} stroke="none" />
      <Polyline
        points={line}
        fill="none"
        stroke={color}
        strokeWidth={1.5}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </Svg>
  );
}

/** One labelled meter row: label + current value on top, its sparkline below. */
function MeterRow({ meter, data }: Readonly<{ meter: PlayerMeter; data: number[] }>) {
  const color = meter.color ?? 'rgba(244,243,240,0.7)';
  return (
    <Box gap={4}>
      <StatRow label={meter.label} value={meter.display} />
      <Sparkline data={data} color={color} />
    </Box>
  );
}

/** One label / value line. The value is tabular so the numbers stop jittering
 * as they tick. */
function StatRow({ label, value }: Readonly<{ label: string; value: string }>) {
  return (
    <Box row between gap={24}>
      <Txt style={STAT_LABEL} color="rgba(244, 243, 240, 0.5)">
        {label}
      </Txt>
      <Txt style={STAT_VALUE} color="rgba(244, 243, 240, 0.82)">
        {value}
      </Txt>
    </Box>
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
      // Deleting the current key while iterating a Map is well-defined (the
      // iterator simply skips removed entries), so no snapshot copy is needed.
      for (const key of hist.keys()) if (!live.has(key)) hist.delete(key);
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
    <Box
      absolute
      top={100}
      left={34}
      z={20}
      minW={280}
      radius={14}
      borderWidth={1}
      border="rgba(255, 255, 255, 0.1)"
      bg="rgba(8, 8, 11, 0.74)"
      px={22}
      py={18}
    >
      <Box row align="center" between gap={24} mb={12}>
        <Txt style={PANEL_TITLE} color="rgba(244, 243, 240, 0.5)">
          {t('stats.title')}
        </Txt>
        <Pressable onPress={onClose} accessibilityRole="button" accessibilityLabel={t('common.close')}>
          <Box shrink={0} center radius="pill" bg="rgba(255, 255, 255, 0.08)" p={4}>
            <IconClose size={15} color="rgba(244, 243, 240, 0.5)" />
          </Box>
        </Pressable>
      </Box>
      <Box gap={8}>
        {rows.map(([k, val]) => (
          <StatRow key={k} label={k} value={val} />
        ))}
      </Box>
      {meters.length > 0 ? (
        <Box
          gap={12}
          mt={12}
          pt={12}
          style={{ borderTopWidth: 1, borderTopColor: 'rgba(255, 255, 255, 0.08)' }}
        >
          {meters.map((m) => (
            <MeterRow key={m.key} meter={m} data={historyRef.current.get(m.key) ?? []} />
          ))}
        </Box>
      ) : null}
    </Box>
  );
}

const PANEL_TITLE = {
  fontFamily: fonts.ui,
  fontSize: 11,
  fontWeight: '700' as const,
  letterSpacing: 1.76,
  textTransform: 'uppercase' as const,
};

const STAT_LABEL = { fontFamily: fonts.ui, fontSize: 13, fontWeight: '500' as const };

const STAT_VALUE = {
  fontFamily: fonts.ui,
  fontSize: 13,
  fontWeight: '500' as const,
  textAlign: 'right' as const,
  fontVariant: ['tabular-nums' as const],
};
