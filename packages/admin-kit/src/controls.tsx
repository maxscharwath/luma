// Generic, reusable admin controls: segmented control, number field, button,
// and a collapsible disclosure.
import { IconChevronDown, type TablerIcon } from '@tabler/icons-react';
import { type ReactNode, useState } from 'react';

/** A pill segmented control: one selected option among a few, each with an
 * optional sub-label. Generic over the value union. */
export function SegmentedControl<T extends string>({
  value,
  options,
  onChange,
}: Readonly<{
  value: T;
  options: { value: T; label: string; desc?: string }[];
  onChange: (v: T) => void;
}>) {
  return (
    <div className="inline-flex gap-1 rounded-[11px] border border-border-strong bg-surface-2 p-1">
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            onClick={() => onChange(o.value)}
            aria-pressed={active}
            className={`rounded-[8px] px-3.5 py-2 text-left transition-colors ${active ? 'bg-accent-soft' : 'hover:bg-white/5'}`}
          >
            <span
              className={`block text-[13px] font-semibold ${active ? 'text-accent' : 'text-text/75'}`}
            >
              {o.label}
            </span>
            {o.desc ? <span className="block text-[11px] text-dim">{o.desc}</span> : null}
          </button>
        );
      })}
    </div>
  );
}

/** A compact numeric input. */
export function NumberField({
  value,
  step,
  min,
  max,
  onChange,
}: Readonly<{
  value: number;
  step?: number;
  min?: number;
  max?: number;
  onChange: (n: number) => void;
}>) {
  return (
    <input
      type="number"
      value={value}
      step={step}
      min={min}
      max={max}
      onChange={(e) => {
        // A blank field is `Number('') === 0`, which would silently commit 0 and
        // bypass `min` (e.g. clearing Max tokens to retype -> max_tokens:0). Ignore
        // empty/NaN; only commit a real number.
        const raw = e.target.value.trim();
        if (raw === '') return;
        const n = Number(raw);
        if (!Number.isNaN(n)) onChange(n);
      }}
      className="w-32 rounded-[9px] border border-border-strong bg-[#0F0F13] px-3.5 py-2.25 text-[13.5px] font-semibold text-text outline-none focus:border-accent/60"
    />
  );
}

type Variant = 'primary' | 'secondary' | 'danger';

const VARIANT: Record<Variant, string> = {
  primary: 'bg-accent text-accent-ink hover:bg-accent-hover',
  secondary: 'border border-border-strong bg-surface-2 text-text hover:border-accent/50',
  danger: 'border border-[#E8536A]/25 bg-[#E8536A]/10 text-[#E8536A]',
};

/** A button with a variant + optional leading icon. */
export function Button({
  label,
  onClick,
  variant = 'secondary',
  disabled,
  icon: Icon,
}: Readonly<{
  label: string;
  onClick?: () => void;
  variant?: Variant;
  disabled?: boolean;
  icon?: TablerIcon;
}>) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className={`inline-flex items-center gap-2 rounded-md px-4 py-2.5 text-[13.5px] font-bold transition-colors disabled:opacity-50 ${VARIANT[variant]}`}
    >
      {Icon ? <Icon size={16} stroke={2.2} /> : null}
      {label}
    </button>
  );
}

/** A collapsible section with a divider header (e.g. "Advanced"). */
export function Disclosure({
  title,
  defaultOpen = false,
  children,
}: Readonly<{ title: string; defaultOpen?: boolean; children: ReactNode }>) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <section className="mt-4.5 border-t border-border pt-5">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex w-full items-center justify-between text-left"
        aria-expanded={open}
      >
        <span className="text-[15px] font-bold uppercase tracking-wider text-text">{title}</span>
        <IconChevronDown
          size={18}
          stroke={2}
          className={`text-muted transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>
      {open ? <div className="mt-4.5">{children}</div> : null}
    </section>
  );
}
