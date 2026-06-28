import React from 'react';

/** Small quality / status pill. No emoji — text only. */
export function Badge({ tone = '4K', children }) {
  const map = {
    '4K': ['var(--luma-accent)', 'var(--luma-accent-soft)'],
    HDR: ['var(--luma-hdr)', 'rgba(199,146,234,.16)'],
    'H.265': ['var(--luma-h265)', 'rgba(95,211,196,.16)'],
    success: ['var(--luma-success)', 'rgba(70,208,141,.16)'],
    info: ['var(--luma-info)', 'rgba(134,168,255,.16)'],
    neutral: ['rgba(244,243,240,.85)', 'rgba(255,255,255,.08)'],
  };
  const [color, background] = map[tone] || map.neutral;
  return React.createElement(
    'span',
    {
      style: {
        font: '700 11px var(--font-ui)',
        letterSpacing: '.04em',
        padding: '4px 9px',
        borderRadius: 6,
        color,
        background,
      },
    },
    children || tone,
  );
}
