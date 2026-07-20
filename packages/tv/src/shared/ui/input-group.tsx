// TV port of the web client's shadcn-style InputGroup (clients/web/src/shared/
// ui/input-group.tsx): the bordered field OWNS the focus visual a calm accent
// border via focus-within instead of the 10-foot amber ring box the global
// [data-focus]:focus rule would draw around the inner <input> (TvTextEntry
// suppresses that ring with focus:shadow-none). Only the field semantics live
// here; callers keep size / shape / background in className.

import type { ReactNode } from 'react';

/** The bordered field wrapper. Owns the focus visual for its controls. */
export function InputGroup({
  children,
  className = '',
}: Readonly<{ children: ReactNode; className?: string }>) {
  return (
    <div
      className={`flex items-center border border-border-strong transition-colors focus-within:border-accent ${className}`}
    >
      {children}
    </div>
  );
}

/** An icon / text / button sitting inside the field, before or after the entry. */
export function InputGroupAddon({ children }: Readonly<{ children: ReactNode }>) {
  return <span className="flex shrink-0 items-center">{children}</span>;
}
