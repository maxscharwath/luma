// Shared building blocks for the account settings page (`/account`): a card
// shell, labelled text/select inputs, and a small async-save state machine that
// every card reuses to surface saving / saved ✓ / error feedback inline.

import { apiErrorText } from '@luma/core';
import { useT } from '@luma/ui';
import type { InputHTMLAttributes, ReactNode } from 'react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Select as UiSelect } from '#web/shared/ui';

const INPUT =
  'w-full rounded-md border border-border-strong bg-surface-2 px-3.5 py-2.5 text-[14px] text-text outline-none transition-colors placeholder:text-dim focus:border-accent disabled:opacity-60';

/** A titled settings card. */
export function Card({
  title,
  desc,
  children,
}: Readonly<{ title: string; desc?: string; children: ReactNode }>) {
  return (
    <section className="rounded-2xl border border-border bg-surface-2/40 p-6">
      <h2 className="font-display text-[18px] font-bold text-text">{title}</h2>
      {desc ? <p className="mt-1 text-[13px] text-muted">{desc}</p> : null}
      <div className="mt-5 flex flex-col gap-4">{children}</div>
    </section>
  );
}

/** A labelled text input. Extra props pass straight to the `<input>`. */
export function Field({
  label,
  hint,
  ...rest
}: Readonly<{ label: string; hint?: string } & InputHTMLAttributes<HTMLInputElement>>) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[13px] font-semibold text-muted">{label}</span>
      <input className={INPUT} {...rest} />
      {hint ? <span className="text-[12px] text-dim">{hint}</span> : null}
    </label>
  );
}

/** A labelled `<select>` from `{ value, label }` options. */
/** Labelled dropdown backed by the shared styled {@link UiSelect}. */
export function Select({
  label,
  value,
  onChange,
  options,
}: Readonly<{
  label: string;
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: string }[];
}>) {
  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-[13px] font-semibold text-muted">{label}</span>
      <UiSelect value={value} onChange={onChange} options={options} ariaLabel={label} block />
    </div>
  );
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

/** Inline saved ✓ / error text next to a card's save button. */
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
