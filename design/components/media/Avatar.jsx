import React from 'react';

/** Profile / cast avatar — gradient disc with initials (no photo needed). */
export function Avatar({ name = '', size = 64, gradient, radius = '50%' }) {
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0])
    .join('')
    .toUpperCase();
  const g = gradient || 'linear-gradient(135deg,#F4B642,#E8743B)';
  return React.createElement(
    'div',
    {
      style: {
        width: size,
        height: size,
        borderRadius: radius,
        background: g,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        fontFamily: 'var(--font-display)',
        fontWeight: 700,
        fontSize: Math.round(size * 0.42),
        color: 'rgba(255,255,255,.92)',
      },
    },
    initials,
  );
}
