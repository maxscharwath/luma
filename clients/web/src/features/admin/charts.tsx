// Admin-dashboard charts, rendered with Chart.js (react-chartjs-2): a reusable
// multi-series line/area chart (Débit / CPU / RAM) and the stacked films-vs-TV
// history bars. Chart.js owns the plot, axes, grid and tooltips; the legend and
// footer are kept as bespoke React to match the "Admin Serveur" design.

import type { HistoryBucket } from '@kroma/core';
import {
  BarElement,
  CategoryScale,
  Chart as ChartJS,
  type ChartOptions,
  Filler,
  LinearScale,
  LineElement,
  PointElement,
  type ScriptableContext,
  Tooltip,
} from 'chart.js';
import type { ReactNode } from 'react';
import { Bar, Line } from 'react-chartjs-2';
import { C } from '#web/features/admin/ui';
import { formatHours } from '#web/shared/lib/adminFormat';

ChartJS.register(
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  BarElement,
  Filler,
  Tooltip,
);

ChartJS.defaults.font.family =
  'Inter, ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif';

/** Seconds between metric samples on the server (see `server/src/metrics.rs`). */
const SAMPLE_SEC = 1.5;

const GRID = 'rgba(255,255,255,.05)';

/** Convert a `#rrggbb` hex to an `rgba()` string at the given alpha. */
function withAlpha(hex: string, a: number): string {
  const n = Number.parseInt(hex.slice(1), 16);
  return `rgba(${(n >> 16) & 255},${(n >> 8) & 255},${n & 255},${a})`;
}

/** A vertical top→bottom gradient under a filled line (scriptable fill). */
function areaFill(color: string) {
  return (ctx: ScriptableContext<'line'>) => {
    const { chart } = ctx;
    const { ctx: canvas, chartArea } = chart;
    if (!chartArea) return withAlpha(color, 0.18);
    const g = canvas.createLinearGradient(0, chartArea.top, 0, chartArea.bottom);
    g.addColorStop(0, withAlpha(color, 0.35));
    g.addColorStop(1, withAlpha(color, 0));
    return g;
  };
}

/** "MAINTENANT" / "40s" / "1m 20s" for a point N samples back from now. */
function timeAgo(secondsAgo: number): string {
  if (secondsAgo <= 0) return 'MAINTENANT';
  const total = Math.round(secondsAgo);
  const m = Math.floor(total / 60);
  const s = total % 60;
  if (m === 0) return `${s}s`;
  return s === 0 ? `${m}m` : `${m}m ${s}s`;
}

/** Sparse time-ago labels: 7 evenly-spaced ticks, blanks elsewhere. */
function timeLabels(n: number): string[] {
  return Array.from({ length: n }, (_, i) => {
    const tickEvery = (n - 1) / 6;
    const onTick = n <= 7 || Math.abs(i % tickEvery) < 0.5 || i === n - 1;
    return onTick ? timeAgo((n - 1 - i) * SAMPLE_SEC) : '';
  });
}

interface SeriesDef {
  label: string;
  data: number[];
  color: string;
  /** Fill the area under the line (the primary series). */
  fill?: boolean;
}

/** A multi-series line/area chart with a left y-axis, legend and footer. */
export function MetricsChart({
  series,
  max,
  formatValue,
  legend,
  footer,
}: Readonly<{
  series: SeriesDef[];
  max: number;
  /** Renders a y-axis tick value and the tooltip number. */
  formatValue: (v: number) => string;
  legend: { label: string; color: string }[];
  footer?: ReactNode;
}>) {
  const n = Math.max(0, ...series.map((s) => s.data.length));
  const labels = timeLabels(n);

  const data = {
    labels,
    datasets: series.map((s) => ({
      label: s.label,
      data: s.data,
      borderColor: s.color,
      backgroundColor: s.fill ? areaFill(s.color) : 'transparent',
      fill: s.fill ? 'origin' : false,
      borderWidth: 2.6,
      tension: 0.35,
      pointRadius: 0,
      pointHoverRadius: 4,
      pointHoverBackgroundColor: s.color,
      pointHoverBorderColor: '#fff',
    })),
  };

  const options: ChartOptions<'line'> = {
    responsive: true,
    maintainAspectRatio: false,
    animation: false,
    interaction: { mode: 'index', intersect: false },
    scales: {
      x: {
        grid: { display: false },
        border: { display: false },
        ticks: {
          color: (c) => (c.tick.label === 'MAINTENANT' ? '#9b9893' : '#6f6c67'),
          autoSkip: false,
          maxRotation: 0,
          font: { size: 11, weight: 500 },
        },
      },
      y: {
        min: 0,
        max,
        grid: { color: GRID },
        border: { display: false },
        ticks: {
          color: '#6f6c67',
          count: 6,
          font: { size: 11, weight: 500 },
          callback: (v) => formatValue(Number(v)),
        },
      },
    },
    plugins: {
      legend: { display: false },
      tooltip: {
        backgroundColor: 'rgba(18,18,22,.95)',
        borderColor: 'rgba(255,255,255,.1)',
        borderWidth: 1,
        padding: 10,
        cornerRadius: 9,
        titleColor: '#9b9893',
        bodyColor: '#f4f3f0',
        usePointStyle: true,
        callbacks: {
          title: (items) => {
            const i = items[0]?.dataIndex ?? 0;
            return timeAgo((n - 1 - i) * SAMPLE_SEC);
          },
          label: (item) => ` ${item.dataset.label}: ${formatValue(item.parsed.y ?? 0)}`,
        },
      },
    },
  };

  return (
    <div>
      <div className="h-52">
        <Line data={data} options={options} />
      </div>
      <div className="mt-3.5 flex flex-wrap items-center justify-between gap-4">
        <Legend items={legend} />
        {footer ? <span className="text-[12.5px] text-dim">{footer}</span> : null}
      </div>
    </div>
  );
}

/** Stacked weekly bars: films (green) + TV (red). */
export function HistoryBars({ buckets }: Readonly<{ buckets: HistoryBucket[] }>) {
  const totalFilms = buckets.reduce((a, b) => a + b.filmsMs, 0);
  const totalTv = buckets.reduce((a, b) => a + b.tvMs, 0);

  const data = {
    labels: buckets.map((b) => b.label),
    datasets: [
      {
        label: 'FILMS',
        data: buckets.map((b) => b.filmsMs),
        backgroundColor: C.films,
        borderRadius: { topLeft: 6, topRight: 6 },
        borderSkipped: false as const,
      },
      {
        label: 'TV',
        data: buckets.map((b) => b.tvMs),
        backgroundColor: C.tv,
        borderRadius: { topLeft: 6, topRight: 6 },
        borderSkipped: false as const,
      },
    ],
  };

  const options: ChartOptions<'bar'> = {
    responsive: true,
    maintainAspectRatio: false,
    animation: false,
    interaction: { mode: 'index', intersect: false },
    scales: {
      x: {
        stacked: true,
        grid: { display: false },
        border: { color: GRID },
        ticks: { color: '#6f6c67', font: { size: 12, weight: 500 } },
      },
      y: {
        stacked: true,
        beginAtZero: true,
        grid: { color: GRID },
        border: { display: false },
        ticks: {
          color: '#6f6c67',
          count: 6,
          font: { size: 11, weight: 500 },
          callback: (v) => formatHours(Number(v)),
        },
      },
    },
    plugins: {
      legend: { display: false },
      tooltip: {
        backgroundColor: 'rgba(18,18,22,.95)',
        borderColor: 'rgba(255,255,255,.1)',
        borderWidth: 1,
        padding: 10,
        cornerRadius: 9,
        titleColor: '#9b9893',
        bodyColor: '#f4f3f0',
        usePointStyle: true,
        callbacks: {
          label: (item) => ` ${item.dataset.label}: ${formatHours(item.parsed.y ?? 0)}`,
        },
      },
    },
  };

  return (
    <div>
      <div className="h-64">
        <Bar data={data} options={options} />
      </div>
      <div className="mt-3.5 flex flex-wrap items-center justify-between gap-4">
        <Legend
          items={[
            { label: 'FILMS', color: C.films },
            { label: 'TV', color: C.tv },
          ]}
        />
        <span className="text-[12.5px] text-dim">
          Totaux : Films {formatHours(totalFilms)} · TV {formatHours(totalTv)}
        </span>
      </div>
    </div>
  );
}

/** Dot-and-label legend shared by both charts. */
function Legend({ items }: Readonly<{ items: { label: string; color: string }[] }>) {
  return (
    <div className="flex flex-wrap gap-4.5">
      {items.map((l) => (
        <span
          key={l.label}
          className="inline-flex items-center gap-1.75 text-[12px] font-semibold tracking-[.06em] text-muted"
        >
          <span className="h-2.25 w-2.25 rounded-full" style={{ background: l.color }} />
          {l.label}
        </span>
      ))}
    </div>
  );
}
