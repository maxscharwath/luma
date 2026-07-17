// The request status chip, one component for every surface: poster-card
// overlay (`card`), table/list rows (`row`) and the discover-detail hero
// (`hero`). While downloading it shows a circular progress ring instead of the
// status dot, so the percentage reads at a glance on a poster.

import type { RequestStatus } from '@kroma/core';
import { useT } from '@kroma/ui';
import { requestStatusMeta } from '#web/features/requests/status';

/** A thin circular progress ring (0..1). Starts at 12 o'clock, fills clockwise. */
function Ring({ value, size, color }: Readonly<{ value: number; size: number; color: string }>) {
  const sw = size <= 12 ? 2 : 2.5;
  const r = (size - sw) / 2;
  const circ = 2 * Math.PI * r;
  const offset = circ * (1 - Math.max(0, Math.min(1, value)));
  const c = size / 2;
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className="-rotate-90 shrink-0"
      aria-hidden="true"
    >
      <circle
        cx={c}
        cy={c}
        r={r}
        fill="none"
        stroke="currentColor"
        strokeWidth={sw}
        opacity={0.28}
      />
      <circle
        cx={c}
        cy={c}
        r={r}
        fill="none"
        stroke={color}
        strokeWidth={sw}
        strokeLinecap="round"
        strokeDasharray={circ}
        strokeDashoffset={offset}
        className="transition-[stroke-dashoffset] duration-500"
      />
    </svg>
  );
}

export function RequestStatusChip({
  status,
  size = 'row',
  progress,
}: Readonly<{
  status: RequestStatus;
  size?: 'card' | 'row' | 'hero';
  /** 0..1 while downloading (page-scoped live events / detail snapshot). */
  progress?: number | null;
}>) {
  const t = useT();
  const m = requestStatusMeta(status);
  const downloading = status === 'downloading' && progress != null;
  const pct = downloading ? `${Math.round((progress ?? 0) * 100)}%` : null;

  // The leading glyph: a progress ring while downloading, else the status dot.
  // Sizes vary by surface (card / hero / row-default).
  let dotSize = 'h-1.5 w-1.5';
  let ringSize = 13;
  if (size === 'card') {
    dotSize = 'h-1 w-1';
    ringSize = 11;
  } else if (size === 'hero') {
    dotSize = 'h-2 w-2';
    ringSize = 16;
  }
  const lead = downloading ? (
    <Ring value={progress ?? 0} size={ringSize} color={m.dot} />
  ) : (
    <span
      className={`${dotSize} rounded-full ${m.pulse ? 'animate-pulse' : ''}`}
      style={{ background: m.dot }}
    />
  );

  if (size === 'card') {
    return (
      <span
        className="inline-flex items-center gap-1 rounded-full px-2 py-[3px] text-[9.5px] font-bold uppercase tracking-[.06em] backdrop-blur-[6px]"
        style={{ color: m.color, background: m.bg }}
      >
        {lead}
        {pct ?? t(m.labelKey)}
      </span>
    );
  }

  if (size === 'hero') {
    return (
      <span
        className="inline-flex items-center gap-2 self-start rounded-full px-4 py-2 text-[13.5px] font-bold"
        style={{ color: m.color, background: m.bg }}
      >
        {lead}
        {t(m.labelKey)}
        {pct ? <span className="tabular-nums">{pct}</span> : null}
      </span>
    );
  }

  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-full px-[11px] py-[5px] text-[11.5px] font-bold"
      style={{ color: m.color, background: m.bg }}
    >
      {lead}
      {t(m.labelKey)}
      {pct ? <span className="tabular-nums">{pct}</span> : null}
    </span>
  );
}
