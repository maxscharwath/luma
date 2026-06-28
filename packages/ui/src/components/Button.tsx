import type { ButtonHTMLAttributes, CSSProperties, ReactNode } from 'react';

export type ButtonVariant = 'primary' | 'glass' | 'ghost';
export type ButtonSize = 'sm' | 'md' | 'lg';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: ReactNode;
  children?: ReactNode;
}

const SIZES: Record<ButtonSize, CSSProperties> = {
  sm: { fontSize: 13, fontWeight: 600, padding: '9px 16px' },
  md: { fontSize: 16, fontWeight: 700, padding: '14px 28px' },
  lg: { fontSize: 19, fontWeight: 700, padding: '17px 38px' },
};

const VARIANTS: Record<ButtonVariant, CSSProperties> = {
  primary: { background: 'var(--luma-accent)', color: 'var(--luma-accent-ink)', border: 'none' },
  glass: {
    background: 'rgba(255,255,255,.1)',
    color: 'var(--luma-text)',
    border: '1px solid var(--luma-border-strong)',
  },
  ghost: { background: 'transparent', color: 'var(--luma-text)', border: 'none' },
};

/** LUMA action button — amber primary / translucent glass / borderless ghost. */
export function Button({
  variant = 'primary',
  size = 'md',
  icon = null,
  children,
  style,
  ...rest
}: Readonly<ButtonProps>) {
  return (
    <button
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 9,
        fontFamily: 'var(--font-ui)',
        borderRadius: 'var(--radius-md)',
        cursor: 'pointer',
        transition: 'transform .14s var(--ease-spring), background .2s, box-shadow .2s',
        ...SIZES[size],
        ...VARIANTS[variant],
        ...style,
      }}
      {...rest}
    >
      {icon}
      {children}
    </button>
  );
}
