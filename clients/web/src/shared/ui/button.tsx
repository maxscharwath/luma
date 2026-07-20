import type { ButtonHTMLAttributes, ReactNode } from 'react';

export type ButtonVariant = 'primary' | 'glass' | 'ghost';
export type ButtonSize = 'sm' | 'md' | 'lg';

const VARIANTS: Record<ButtonVariant, string> = {
  primary: 'bg-accent text-accent-ink font-bold hover:bg-accent-hover',
  glass: 'bg-white/10 text-text border border-border-strong hover:bg-white/15',
  ghost: 'bg-transparent text-text hover:bg-white/5',
};

const SIZES: Record<ButtonSize, string> = {
  sm: 'text-sm px-3.5 py-2',
  md: 'text-base px-6 py-3',
  lg: 'text-lg px-8 py-3.5',
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: ReactNode;
}

export function Button({
  variant = 'primary',
  size = 'md',
  icon,
  className = '',
  children,
  ...rest
}: Readonly<ButtonProps>) {
  return (
    <button
      type="button"
      className={`inline-flex items-center justify-center gap-2 rounded-md font-semibold cursor-pointer
        transition-[transform,background-color,box-shadow] duration-200 ease-spring
        active:scale-95 disabled:opacity-50 disabled:pointer-events-none
        ${VARIANTS[variant]} ${SIZES[size]} ${className}`}
      {...rest}
    >
      {icon}
      {children}
    </button>
  );
}
