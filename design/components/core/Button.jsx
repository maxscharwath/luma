import React from 'react';

/** LUMA primary action button. */
export function Button({
  variant = 'primary',
  size = 'md',
  icon = null,
  children,
  style = {},
  ...rest
}) {
  const sizes = {
    sm: { fontSize: 13, fontWeight: 600, padding: '9px 16px' },
    md: { fontSize: 16, fontWeight: 700, padding: '14px 28px' },
    lg: { fontSize: 19, fontWeight: 700, padding: '17px 38px' },
  };
  const variants = {
    primary: { background: 'var(--luma-accent)', color: 'var(--luma-accent-ink)', border: 'none' },
    glass: {
      background: 'rgba(255,255,255,.1)',
      color: 'var(--luma-text)',
      border: '1px solid var(--luma-border-strong)',
    },
    ghost: { background: 'transparent', color: 'var(--luma-text)', border: 'none' },
  };
  const s = sizes[size] || sizes.md;
  return React.createElement(
    'button',
    {
      style: {
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 9,
        fontFamily: 'var(--font-ui)',
        fontSize: s.fontSize,
        fontWeight: s.fontWeight,
        padding: s.padding,
        borderRadius: 'var(--radius-md)',
        cursor: 'pointer',
        transition: 'transform .14s var(--ease-spring), background .2s, box-shadow .2s',
        ...(variants[variant] || variants.primary),
        ...style,
      },
      ...rest,
    },
    icon,
    children,
  );
}
