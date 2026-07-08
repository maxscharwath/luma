// Form + modal primitives for the admin console (selects, inputs, modal scaffold,
// labelled fields, modal action footer). Split out of `ui.tsx`; the display
// primitives there re-export these so call sites keep importing everything from
// `#web/features/admin/ui`.
import type { ReactNode } from 'react';
import { Select as UiSelect } from '#web/shared/ui';

/** Admin value-chip select (label === value), backed by the shared styled
 * {@link UiSelect}. Keeps the string[] API its call sites already use. */
export function Select({
  value,
  options,
  onChange,
}: Readonly<{
  value: string;
  options: string[];
  onChange?: (v: string) => void;
}>) {
  // Keep the current value selectable even if it isn't in the list (empty values
  // aren't valid Radix items, so they just fall through to the placeholder).
  const all = value && !options.includes(value) ? [value, ...options] : options;
  return (
    <UiSelect
      value={value}
      onChange={(v) => onChange?.(v)}
      options={all.map((o) => ({ value: o, label: o }))}
    />
  );
}

export function TextInput({
  value,
  onChange,
  onBlur,
  placeholder,
  className = '',
  type = 'text',
}: Readonly<{
  value: string;
  onChange?: (v: string) => void;
  onBlur?: () => void;
  placeholder?: string;
  className?: string;
  /** Input type, e.g. `password` for secrets. Defaults to `text`. */
  type?: string;
}>) {
  return (
    <input
      type={type}
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange?.(e.target.value)}
      onBlur={onBlur}
      className={`min-w-50 rounded-[9px] border border-border-strong bg-[#0F0F13] px-3.5 py-2.25 text-[13.5px] font-semibold text-text outline-none focus:border-accent/60 ${className}`}
    />
  );
}

/** Centered modal overlay (click-outside to close). */
export function Modal({
  title,
  children,
  onClose,
}: Readonly<{
  title: string;
  children: ReactNode;
  onClose: () => void;
}>) {
  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="w-full max-w-115 rounded-2xl border border-border bg-surface-1 p-6 shadow-pop"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
      >
        <div className="mb-4 font-display text-[20px] font-bold">{title}</div>
        {children}
      </div>
    </div>
  );
}

/** A labelled form field (uppercase caption + control + optional hint below). */
export function Field({
  label,
  hint,
  children,
}: Readonly<{ label: string; hint?: string; children: ReactNode }>) {
  return (
    <div className="mb-4">
      <span className="mb-1.5 block text-[12px] font-bold uppercase tracking-[.12em] text-dim">
        {label}
      </span>
      {children}
      {hint ? <p className="mt-1.5 text-[12px] leading-relaxed text-dim">{hint}</p> : null}
    </div>
  );
}

/** The standard modal footer: a right-aligned cancel + primary pair, with an
 * optional destructive action pinned left (e.g. "Delete account"). The caller
 * passes the already-resolved `confirmLabel` (so it can swap to "Saving…"). */
export function ModalActions({
  onCancel,
  cancelLabel,
  onConfirm,
  confirmLabel,
  busy,
  disabled,
  destructive,
}: Readonly<{
  onCancel: () => void;
  cancelLabel: string;
  onConfirm: () => void;
  confirmLabel: string;
  busy?: boolean;
  disabled?: boolean;
  destructive?: { label: string; onClick: () => void; disabled?: boolean; title?: string };
}>) {
  return (
    <div
      className={`mt-5 flex items-center gap-3 ${destructive ? 'justify-between' : 'justify-end'}`}
    >
      {destructive ? (
        <button
          type="button"
          onClick={destructive.onClick}
          disabled={busy || destructive.disabled}
          title={destructive.title}
          className="text-[13px] font-semibold text-[#E8536A] disabled:opacity-40"
        >
          {destructive.label}
        </button>
      ) : null}
      <div className="flex gap-2.5">
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-4 py-2.5 text-[14px] font-semibold text-muted"
        >
          {cancelLabel}
        </button>
        <button
          type="button"
          onClick={onConfirm}
          disabled={busy || disabled}
          className="rounded-md bg-accent px-5 py-2.5 text-[14px] font-bold text-accent-ink disabled:opacity-50"
        >
          {confirmLabel}
        </button>
      </div>
    </div>
  );
}
