// A styled dropdown select built on Radix Select the app-wide replacement for
// native <select>. Renders as the design's chevron value-chip; the popup lists
// options with a check on the active one. Keyboard + a11y come from Radix.
//
// Note: Radix forbids an empty-string option value. Use a non-empty sentinel
// (e.g. "none") for an "unset" choice and map it at the call site.

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

export interface SelectProps {
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

export function Select({
  value,
  onChange,
  options,
  placeholder,
  className = '',
  ariaLabel,
  disabled,
  block,
}: Readonly<SelectProps>) {
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
