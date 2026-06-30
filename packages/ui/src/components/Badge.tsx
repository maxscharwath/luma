import type { ReactNode } from 'react';

export type BadgeTone = '4K' | 'HDR' | 'H.265' | 'success' | 'info' | 'neutral';

export interface BadgeProps {
  tone?: BadgeTone;
  children?: ReactNode;
}

// [color, background] per tone.
const MAP: Record<BadgeTone, [string, string]> = {
  '4K': ['var(--luma-accent)', 'var(--luma-accent-soft)'],
  HDR: ['var(--luma-hdr)', 'rgba(199,146,234,.16)'],
  'H.265': ['var(--luma-h265)', 'rgba(95,211,196,.16)'],
  success: ['var(--luma-success)', 'rgba(70,208,141,.16)'],
  info: ['var(--luma-info)', 'rgba(134,168,255,.16)'],
  neutral: ['rgba(244,243,240,.85)', 'rgba(255,255,255,.08)'],
};

/** Small quality / status pill. Text only never emoji (FR/EN/4K/HDR/H.265). */
export function Badge({ tone = '4K', children }: Readonly<BadgeProps>) {
  const [color, background] = MAP[tone] ?? MAP.neutral;
  return (
    <span
      style={{
        font: '700 11px var(--font-ui)',
        letterSpacing: '.04em',
        padding: '4px 9px',
        borderRadius: 6,
        color,
        background,
      }}
    >
      {children ?? tone}
    </span>
  );
}
