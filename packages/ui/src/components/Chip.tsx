import type { HTMLAttributes, ReactNode } from 'react';

export interface ChipProps extends HTMLAttributes<HTMLSpanElement> {
  active?: boolean;
  children?: ReactNode;
}

/** Pill chip language codes, audio formats, filters. */
export function Chip({ active = false, children, style, ...rest }: Readonly<ChipProps>) {
  return (
    <span
      style={{
        font: '600 13px var(--font-ui)',
        color: active ? 'var(--kroma-accent-ink)' : 'var(--kroma-text)',
        background: active ? 'var(--kroma-accent)' : 'rgba(255,255,255,.07)',
        border: '1px solid var(--kroma-border)',
        padding: '6px 14px',
        borderRadius: 'var(--radius-pill)',
        cursor: 'pointer',
        ...style,
      }}
      {...rest}
    >
      {children}
    </span>
  );
}
