// Form + modal primitives for the admin console: the styled Radix select (both
// an option-object API and a plain string[] value-chip wrapper), text input,
// modal scaffold, labelled fields, and the modal action footer. Self-contained
// (own copy of the styled select) so the kit needs no app import.

import * as RSelect from '@radix-ui/react-select';
import { IconCheck, IconChevronDown } from '@tabler/icons-react';
import type { ReactNode } from 'react';

export interface SelectOption {
  value: string;
  label: ReactNode;
  /** Plain-text form for typeahead + the trigger when selected (defaults to
   * `label` when it is a string). Required when `label` is not a string. */
  text?: string;
  disabled?: boolean;
}

export interface OptionSelectProps {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  placeholder?: string;
  /** Extra classes for the trigger chip (e.g. width). */
  className?: string;
  /** Accessible label when there's no visible <label> wrapping the control. */
  ariaLabel?: string;
  disabled?: boolean;
  /** Stretch the trigger to fill its container. */
  block?: boolean;
}

/** A styled dropdown built on Radix Select (rich option objects). Radix forbids
 * an empty-string option value: use a non-empty sentinel and map it at the call
 * site. */
export function OptionSelect({
  value,
  onChange,
  options,
  placeholder,
  className = '',
  ariaLabel,
  disabled,
  block,
}: Readonly<OptionSelectProps>) {
  return (
    <RSelect.Root value={value || undefined} onValueChange={onChange} disabled={disabled}>
      <RSelect.Trigger
        aria-label={ariaLabel}
        className={`inline-flex items-center justify-between gap-2 rounded-md border border-border-strong bg-surface-2 px-3.5 py-2.5 text-[14px] font-medium text-text outline-none transition-colors focus:border-accent data-[placeholder]:text-dim disabled:cursor-not-allowed disabled:opacity-60 ${block ? 'w-full' : ''} ${className}`}
      >
        <span className="truncate">
          <RSelect.Value placeholder={placeholder} />
        </span>
        <RSelect.Icon className="shrink-0 text-dim">
          <IconChevronDown size={14} stroke={2.4} />
        </RSelect.Icon>
      </RSelect.Trigger>

      <RSelect.Portal>
        <RSelect.Content
          position="popper"
          sideOffset={6}
          className="z-100 max-h-[min(60vh,20rem)] w-[var(--radix-select-trigger-width)] min-w-40 overflow-hidden rounded-[11px] border border-border-strong bg-[#121216] shadow-pop"
        >
          <RSelect.Viewport className="p-1.5">
            {options.map((o) => (
              <RSelect.Item
                key={o.value}
                value={o.value}
                disabled={o.disabled}
                textValue={o.text ?? (typeof o.label === 'string' ? o.label : undefined)}
                className="relative flex cursor-pointer select-none items-center rounded-[7px] py-2 pl-3 pr-8 text-[13px] font-medium text-text outline-none data-[disabled]:cursor-not-allowed data-[highlighted]:bg-white/[.06] data-[disabled]:opacity-40 data-[state=checked]:text-accent"
              >
                <RSelect.ItemText>{o.label}</RSelect.ItemText>
                <RSelect.ItemIndicator className="absolute right-2.5">
                  <IconCheck size={14} stroke={2.4} />
                </RSelect.ItemIndicator>
              </RSelect.Item>
            ))}
          </RSelect.Viewport>
        </RSelect.Content>
      </RSelect.Portal>
    </RSelect.Root>
  );
}

/** Admin value-chip select (label === value), backed by {@link OptionSelect}.
 * Keeps the plain string[] API its call sites already use. */
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
    <OptionSelect
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
      className={`min-w-0 rounded-[9px] border border-border-strong bg-[#0F0F13] px-3.5 py-2.25 text-[13.5px] font-semibold text-text outline-none focus:border-accent/60 ${className}`}
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
    // biome-ignore lint/a11y/noStaticElementInteractions: presentational backdrop; the click only dismisses the modal (a mouse convenience). Keyboard users close via the dialog's own Cancel/action buttons.
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-6"
      onClick={onClose}
      role="presentation"
    >
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: the onClick only stops propagation so an inside-click doesn't reach the backdrop; there is no user action to mirror on the keyboard. */}
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
 * passes the already-resolved `confirmLabel` (so it can swap to "Saving..."). */
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
