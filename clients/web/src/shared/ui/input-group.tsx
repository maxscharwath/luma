// A small shadcn-style InputGroup: an icon/button addon + an input share one
// bordered "field", and the FOCUS state lives on the group (a subtle accent
// border via `focus-within`), not on the raw input. This suppresses the app's
// global amber `:focus-visible` ring (packages/ui/styles.css, meant for 10-foot
// TV navigation) on the inner control so web inputs read clean.

import type { InputHTMLAttributes, ReactNode } from 'react';

/** The bordered field wrapper. Owns the focus visual for its controls. */
export function InputGroup({
  children,
  className = '',
}: Readonly<{ children: ReactNode; className?: string }>) {
  return (
    <div
      className={`flex items-center gap-2 rounded-[9px] border border-border-strong bg-surface-2 px-3 transition-colors focus-within:border-accent/60 ${className}`}
    >
      {children}
    </div>
  );
}

/** An icon / text / button sitting inside the field, before or after the input. */
export function InputGroupAddon({ children }: Readonly<{ children: ReactNode }>) {
  return <span className="flex shrink-0 items-center text-muted">{children}</span>;
}

/** The input itself: transparent, borderless, and with its own focus ring removed
 *  (`focus-visible:shadow-none` overrides the global amber ring). */
export function InputGroupInput({
  className = '',
  ...rest
}: Readonly<InputHTMLAttributes<HTMLInputElement>>) {
  return (
    <input
      {...rest}
      className={`w-full min-w-0 bg-transparent py-2 text-[13px] text-text outline-none placeholder:text-muted focus-visible:shadow-none ${className}`}
    />
  );
}
