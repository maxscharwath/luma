// Shared building blocks for the account settings page (`/account`), styled to
// the KROMA "Mon profil" design: overline sections separated by hairlines, flat
// dark panels, uppercase field labels with amber focus, and icon-led preference
// rows. Also exports the small async-save state machine every section reuses.

import { apiErrorText, type MessageKey } from '@kroma/core';
import { useT } from '@kroma/ui';
import type { InputHTMLAttributes, ReactNode } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';

/** A page section: an uppercase overline separated from the previous section by
 * a hairline rule (the first one drops both). */
export function Section({ title, children }: Readonly<{ title: string; children: ReactNode }>) {
  return (
    <section className="mt-6 border-t border-border pt-7 first:mt-0 first:border-none first:pt-0">
      <h2 className="mb-4 text-[13px] font-bold uppercase tracking-[0.08em] text-text">{title}</h2>
      <div className="flex flex-col gap-4">{children}</div>
    </section>
  );
}

/** A flat dark panel (the design's card): hairline border, soft drop shadow. */
export function Panel({
  className = '',
  children,
}: Readonly<{ className?: string; children: ReactNode }>) {
  return (
    <div className={`rounded-xl border border-border bg-surface-1 shadow-card ${className}`}>
      {children}
    </div>
  );
}

/** A labelled input: uppercase 11px label over a dark field with an optional
 * leading adornment (icon or `@`) and an amber focus ring. */
export function LabeledInput({
  label,
  leading,
  className = '',
  ...rest
}: Readonly<{ label: string; leading?: ReactNode } & InputHTMLAttributes<HTMLInputElement>>) {
  return (
    <label className={`flex flex-col gap-2 ${className}`}>
      <span className="text-[11px] font-bold uppercase tracking-[0.08em] text-dim">{label}</span>
      <div className="flex items-center gap-2.5 rounded-md border border-border-strong bg-bg px-3.5 transition-colors focus-within:border-accent">
        {leading}
        <input
          className="min-w-0 flex-1 bg-transparent py-3 text-[14.5px] font-semibold text-text outline-none placeholder:text-dim"
          {...rest}
        />
      </div>
    </label>
  );
}

/** A preference row inside a {@link Panel}: an amber-tinted icon, a label with a
 * muted description, and a right-aligned control (a select or a button). */
export function PrefRow({
  icon,
  label,
  desc,
  control,
}: Readonly<{ icon: ReactNode; label: string; desc: string; control: ReactNode }>) {
  return (
    <div className="flex items-center justify-between gap-5 px-5.5 py-4">
      <div className="flex min-w-0 items-center gap-3.5">
        <span className="flex size-8.5 flex-none items-center justify-center rounded-lg bg-accent-soft text-accent">
          {icon}
        </span>
        <div className="min-w-0">
          <div className="text-[14.5px] font-bold text-text">{label}</div>
          <div className="mt-0.5 text-[12.5px] text-muted">{desc}</div>
        </div>
      </div>
      <div className="flex-none">{control}</div>
    </div>
  );
}

export type Strength = {
  score: 0 | 1 | 2 | 3 | 4;
  width: string;
  color: string;
  labelKey: MessageKey | null;
};

/** A rough password-strength estimate for the meter: one point each for length
 * (8+), mixed case, a digit and a symbol. Purely a UI hint the server enforces
 * the real minimum. */
export function passwordStrength(pw: string): Strength {
  if (!pw) return { score: 0, width: '0%', color: 'transparent', labelKey: null };
  let s = 0;
  if (pw.length >= 8) s += 1;
  if (/[a-z]/.test(pw) && /[A-Z]/.test(pw)) s += 1;
  if (/\d/.test(pw)) s += 1;
  if (/[^A-Za-z0-9]/.test(pw)) s += 1;
  const score = Math.max(1, s) as 1 | 2 | 3 | 4;
  const map = {
    1: { width: '25%', color: 'var(--kroma-danger)', labelKey: 'account.passwordStrengthWeak' },
    2: { width: '50%', color: '#E8A23B', labelKey: 'account.passwordStrengthFair' },
    3: { width: '75%', color: 'var(--kroma-info)', labelKey: 'account.passwordStrengthGood' },
    4: { width: '100%', color: 'var(--kroma-success)', labelKey: 'account.passwordStrengthStrong' },
  } as const;
  return { score, ...map[score] };
}

export type SaveStatus = 'idle' | 'saving' | 'saved' | 'error';

/** Async-save state machine: `run(fn, fallbackMsg)` flips to `saving`, then
 * `saved` (auto-clearing after ~2.5s) or `error` with the server's message. */
export function useSave() {
  const [status, setStatus] = useState<SaveStatus>('idle');
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const run = useCallback(async (fn: () => Promise<void>, fallback: string) => {
    if (timer.current) clearTimeout(timer.current);
    setStatus('saving');
    setError(null);
    try {
      await fn();
      setStatus('saved');
      timer.current = setTimeout(() => setStatus('idle'), 2500);
    } catch (e) {
      setError(apiErrorText(e, fallback));
      setStatus('error');
    }
  }, []);

  return { status, error, run };
}

/** Inline saved ✓ / error text next to a section's save button. */
export function StatusText({
  status,
  error,
}: Readonly<{ status: SaveStatus; error: string | null }>) {
  const t = useT();
  if (status === 'saved')
    return <span className="text-[13px] font-medium text-accent">{t('common.saved')}</span>;
  if (status === 'error')
    return <span className="text-[13px] font-medium text-danger">{error}</span>;
  return null;
}
