import React from 'react';

/** Pill chip — language codes, audio formats, filters. */
export function Chip({ active = false, children, ...rest }) {
  return React.createElement(
    'span',
    {
      style: {
        font: '600 13px var(--font-ui)',
        color: active ? 'var(--luma-accent-ink)' : 'var(--luma-text)',
        background: active ? 'var(--luma-accent)' : 'rgba(255,255,255,.07)',
        border: '1px solid var(--luma-border)',
        padding: '6px 14px',
        borderRadius: 'var(--radius-pill)',
        cursor: 'pointer',
      },
      ...rest,
    },
    children,
  );
}
