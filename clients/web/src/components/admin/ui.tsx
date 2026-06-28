// Shared presentational primitives for the admin console, matching the
// "Admin Serveur" design (cards, titled sections, stat cards, toggle/select
// rows, progress bars, gradient avatars).
import { IconChevronDown } from '@tabler/icons-react';
import type { ReactNode } from 'react';
import { avatarGradient, initial } from '#web/lib/adminFormat';
import { imageUrl } from '#web/lib/api';

/** Chart/semantic colors from the design that aren't Tailwind tokens. */
export const C = {
  accent: '#F4B642',
  green: '#46D08D',
  red: '#E8536A',
  blue: '#5C8DF6',
  purple: '#C792EA',
  films: '#84CE7E',
  tv: '#E8536A',
  cpuRed: '#E5566B',
} as const;

export function Card({
  children,
  className = '',
}: Readonly<{ children: ReactNode; className?: string }>) {
  return (
    <div className={`rounded-2xl border border-border bg-surface-1 shadow-card ${className}`}>
      {children}
    </div>
  );
}

/** A titled dashboard section with a top divider and an optional right slot. */
export function Section({
  title,
  right,
  children,
}: Readonly<{
  title: string;
  right?: ReactNode;
  children: ReactNode;
}>) {
  return (
    <section className="mt-4.5 border-t border-border pt-6.5">
      <div className="mb-4.5 flex items-center justify-between gap-3">
        <h2 className="text-[15px] font-bold uppercase tracking-wider text-text">{title}</h2>
        {right}
      </div>
      {children}
    </section>
  );
}

/** A muted, chevroned "filter" label (display-only, like the design). */
export function FilterLabel({ children }: Readonly<{ children: ReactNode }>) {
  return (
    <span className="inline-flex cursor-default items-center gap-1.5 text-[14px] font-semibold text-muted">
      {children}
      <IconChevronDown size={11} stroke={2.5} />
    </span>
  );
}

export function StatCard({
  label,
  value,
  unit,
  color = 'var(--luma-text)',
}: Readonly<{
  label: string;
  value: ReactNode;
  unit?: string;
  color?: string;
}>) {
  return (
    <Card className="px-5.5 py-5">
      <div className="text-[9.5px] font-bold uppercase tracking-[.13em] text-dim">{label}</div>
      <div className="mt-2.5 flex items-baseline gap-2">
        <span className="font-display text-[30px] font-bold" style={{ color }}>
          {value}
        </span>
        {unit ? <span className="text-[13px] text-dim">{unit}</span> : null}
      </div>
    </Card>
  );
}

/** The 46×26 pill switch from the design. */
export function Toggle({
  on,
  onChange,
}: Readonly<{ on: boolean; onChange?: (v: boolean) => void }>) {
  return (
    <button
      type="button"
      onClick={() => onChange?.(!on)}
      className="relative h-6.5 w-11.5 shrink-0 rounded-full transition-colors"
      style={{ background: on ? C.green : 'rgba(255,255,255,.14)' }}
      aria-pressed={on}
    >
      <span
        className="absolute left-0.75 top-0.75 h-5 w-5 rounded-full bg-white shadow-[0_2px_4px_rgba(0,0,0,.4)] transition-transform"
        style={{ transform: on ? 'translateX(20px)' : 'translateX(0)' }}
      />
    </button>
  );
}

/** Styled native select rendered as the design's value chip. */
export function Select({
  value,
  options,
  onChange,
}: Readonly<{
  value: string;
  options: string[];
  onChange?: (v: string) => void;
}>) {
  const opts = options.length || options.includes(value) ? options : [value];
  return (
    <span className="relative inline-flex items-center">
      <select
        value={value}
        onChange={(e) => onChange?.(e.target.value)}
        className="cursor-pointer appearance-none rounded-[9px] border border-border-strong bg-surface-2 py-2.25 pl-3.25 pr-9 text-[13.5px] font-semibold text-text outline-none"
      >
        {(opts.includes(value) ? opts : [value, ...opts]).map((o) => (
          <option key={o} value={o}>
            {o}
          </option>
        ))}
      </select>
      <IconChevronDown className="pointer-events-none absolute right-3" size={13} stroke={2.5} />
    </span>
  );
}

export function TextInput({
  value,
  onChange,
  onBlur,
  placeholder,
  className = '',
}: Readonly<{
  value: string;
  onChange?: (v: string) => void;
  onBlur?: () => void;
  placeholder?: string;
  className?: string;
}>) {
  return (
    <input
      value={value}
      placeholder={placeholder}
      onChange={(e) => onChange?.(e.target.value)}
      onBlur={onBlur}
      className={`min-w-50 rounded-[9px] border border-border-strong bg-[#0F0F13] px-3.5 py-2.25 text-[13.5px] font-semibold text-text outline-none focus:border-accent/60 ${className}`}
    />
  );
}

export function ProgressBar({
  pct,
  color = C.accent,
  height = 6,
}: Readonly<{
  pct: number;
  color?: string;
  height?: number;
}>) {
  return (
    <div className="w-full overflow-hidden rounded-full bg-white/8" style={{ height }}>
      <div
        className="h-full rounded-full"
        style={{ width: `${Math.max(0, Math.min(100, pct))}%`, background: color }}
      />
    </div>
  );
}

export function Pill({
  children,
  color = 'var(--luma-text)',
  bg = 'transparent',
}: Readonly<{
  children: ReactNode;
  color?: string;
  bg?: string;
}>) {
  return (
    <span
      className="inline-flex items-center gap-1.5 rounded-[7px] px-2.25 py-0.75 text-[13px] font-semibold"
      style={{ color, background: bg }}
    >
      {children}
    </span>
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

/** A labelled form field (uppercase caption + control). */
export function Field({ label, children }: Readonly<{ label: string; children: ReactNode }>) {
  return (
    <div className="mb-4">
      <label className="mb-1.5 block text-[12px] font-bold uppercase tracking-[.12em] text-dim">
        {label}
      </label>
      {children}
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
    <div className={`mt-5 flex items-center gap-3 ${destructive ? 'justify-between' : 'justify-end'}`}>
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

/** Gradient avatar with initial fallback (or a cached image when present). */
export function Avatar({
  name,
  avatarUrl,
  size = 42,
  radius,
}: Readonly<{
  name: string;
  avatarUrl?: string | null;
  size?: number;
  radius?: number;
}>) {
  const r = radius ?? size / 2;
  const url = imageUrl(avatarUrl ?? undefined);
  return (
    <span
      className="flex shrink-0 items-center justify-center overflow-hidden bg-cover bg-center font-display font-bold text-white/95"
      style={{
        width: size,
        height: size,
        borderRadius: r,
        background: url ? undefined : avatarGradient(name),
        backgroundImage: url ? `url(${url})` : undefined,
        fontSize: size * 0.42,
      }}
    >
      {url ? '' : initial(name)}
    </span>
  );
}
