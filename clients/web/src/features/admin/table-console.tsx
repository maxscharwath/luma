// Shared building blocks for the admin "console" pages (Pipeline + Requests
// queue). Both render the same searchable table shell: a search box, filter
// chips, column heads, a count summary, a floating toast and an event-driven,
// throttled reload. These live here once instead of being copy/pasted per page.

import { IconSearch, IconX } from '@tabler/icons-react';
import { type ReactNode, useCallback, useRef, useState } from 'react';
import { InputGroup, InputGroupAddon, InputGroupInput } from '#web/shared/ui/input-group';

/** Coalesce event-driven reloads to at most one call per 1.5 s. */
export function useThrottledReload(reload: () => void): () => void {
  const lastReloadRef = useRef(0);
  return useCallback(() => {
    const now = Date.now();
    if (now - lastReloadRef.current < 1500) return;
    lastReloadRef.current = now;
    reload();
  }, [reload]);
}

/** Toast state + a `flash(text)` helper that shows it for 2.8 s. */
export function useConsoleToast(): {
  toast: { text: string; on: boolean };
  flash: (text: string) => void;
} {
  const [toast, setToast] = useState<{ text: string; on: boolean }>({ text: '', on: false });
  const flash = (text: string) => {
    setToast({ text, on: true });
    window.setTimeout(() => setToast((s) => ({ ...s, on: false })), 2800);
  };
  return { toast, flash };
}

/** The header search box with a clear button. */
export function ConsoleSearch({
  value,
  onChange,
  placeholder,
}: Readonly<{ value: string; onChange: (v: string) => void; placeholder: string }>) {
  return (
    <div className="w-80 max-w-full">
      <InputGroup className="h-11">
        <InputGroupAddon>
          <IconSearch size={17} />
        </InputGroupAddon>
        <InputGroupInput
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={placeholder}
          className="text-[14px] font-semibold"
        />
        {value ? (
          <button
            type="button"
            onClick={() => onChange('')}
            className="shrink-0 text-white/50 hover:text-white"
          >
            <IconX size={16} stroke={2.2} />
          </button>
        ) : null}
      </InputGroup>
    </div>
  );
}

/** The "N tracked · M need action" summary line above the table. */
export function ConsoleSummary({
  total,
  totalLabel,
  accent,
  accentLabel,
}: Readonly<{ total: number; totalLabel: string; accent: number; accentLabel: string }>) {
  return (
    <p className="mb-5 mt-1.5 text-[14.5px] font-medium text-dim">
      <span className="font-bold text-white">{total.toLocaleString()}</span>{' '}
      {totalLabel} ·{' '}
      <span className="font-bold text-accent">{accent.toLocaleString()}</span>{' '}
      {accentLabel}
    </p>
  );
}

/** Column heading cell for the table header row. */
export function Head({
  children,
  className = '',
}: Readonly<{ children: ReactNode; className?: string }>) {
  return (
    <span
      className={`text-[9.5px] font-bold uppercase tracking-[.12em] text-white/40 ${className}`}
    >
      {children}
    </span>
  );
}

/** A filter chip. `tone` picks the active accent (defaults to the pink accent). */
export function Chip({
  label,
  count,
  dot,
  on,
  tone = 'accent',
  onClick,
}: Readonly<{
  label: string;
  count?: number;
  dot?: string;
  on: boolean;
  tone?: 'accent' | 'blue';
  onClick: () => void;
}>) {
  const active =
    tone === 'accent'
      ? 'border-accent/35 bg-accent/[0.14] text-accent'
      : 'border-[#86A8FF]/35 bg-[#86A8FF]/[0.14] text-[#86A8FF]';
  return (
    <button
      type="button"
      onClick={onClick}
      className={`inline-flex items-center gap-2 rounded-full border px-3.5 py-2 text-[13px] font-semibold transition-colors ${on ? active : 'border-white/[0.08] bg-[#15151A] text-white/65'}`}
    >
      {dot ? <span className="h-[7px] w-[7px] rounded-full" style={{ background: dot }} /> : null}
      {label}
      {count != null ? (
        <span className="tabular-nums opacity-60">{count.toLocaleString()}</span>
      ) : null}
    </button>
  );
}

/** The floating bottom-center toast, driven by `useConsoleToast`. */
export function ConsoleToast({ toast }: Readonly<{ toast: { text: string; on: boolean } }>) {
  return (
    <div
      className="pointer-events-none fixed bottom-6 left-1/2 z-[80] -translate-x-1/2 transition-all duration-200"
      style={{
        opacity: toast.on ? 1 : 0,
        transform: `translateX(-50%) translateY(${toast.on ? 0 : 12}px)`,
      }}
    >
      <div className="inline-flex items-center gap-2.5 rounded-full border border-white/12 bg-[#1C1C22] px-[18px] py-2.5 shadow-[0_20px_50px_rgba(0,0,0,.55)]">
        <span className="h-2 w-2 flex-[0_0_8px] rounded-full bg-accent" />
        <span className="text-[13.5px] font-semibold text-white">{toast.text}</span>
      </div>
    </div>
  );
}
