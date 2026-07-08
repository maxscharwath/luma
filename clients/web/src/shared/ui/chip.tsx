import type { HTMLAttributes, ReactNode } from 'react';

export interface ChipProps extends HTMLAttributes<HTMLSpanElement> {
  active?: boolean;
  children?: ReactNode;
}

export function Chip({ active = false, className = '', children, ...rest }: Readonly<ChipProps>) {
  return (
    <span
      className={`inline-block rounded-full px-3.5 py-1.5 text-[13px] font-semibold
        ${active ? 'bg-accent text-accent-ink' : 'bg-white/8 text-text'} ${className}`}
      {...rest}
    >
      {children}
    </span>
  );
}
