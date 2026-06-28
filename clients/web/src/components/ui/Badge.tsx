import type { ReactNode } from 'react';

export type BadgeTone = '4K' | 'HDR' | 'H.265' | 'success' | 'info' | 'neutral';

const TONES: Record<BadgeTone, string> = {
  '4K': 'text-accent bg-accent-soft',
  HDR: 'text-hdr bg-[rgba(199,146,234,.16)]',
  'H.265': 'text-h265 bg-[rgba(95,211,196,.16)]',
  success: 'text-success bg-[rgba(70,208,141,.16)]',
  info: 'text-info bg-[rgba(134,168,255,.16)]',
  neutral: 'text-text/85 bg-white/8',
};

export interface BadgeProps {
  tone?: BadgeTone;
  children?: ReactNode;
}

export function Badge({ tone = '4K', children }: Readonly<BadgeProps>) {
  return (
    <span
      className={`inline-block rounded-md px-1.75 py-0.75 text-[11px] font-bold leading-none ${TONES[tone]}`}
    >
      {children}
    </span>
  );
}
