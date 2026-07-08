// A searchable Radix Select the model picker on the IA page. Renders as the
// design's chevron value-chip; opening reveals a sticky search box that filters
// the options (model lists from Ollama/OpenRouter get long). The current value
// is always selectable even if it isn't in the loaded list.
import * as Select from '@radix-ui/react-select';
import { IconCheck, IconChevronDown, IconSearch } from '@tabler/icons-react';
import { useEffect, useMemo, useRef, useState } from 'react';

export function SearchSelect({
  value,
  options,
  onChange,
  placeholder,
  searchPlaceholder,
  className = '',
}: Readonly<{
  value: string;
  options: string[];
  onChange?: (v: string) => void;
  placeholder?: string;
  searchPlaceholder?: string;
  className?: string;
}>) {
  const [open, setOpen] = useState(false);
  const [q, setQ] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  // Radix Select focuses the selected item on open; pull focus to the search
  // box instead (Select.Content has no onOpenAutoFocus, unlike Popover).
  useEffect(() => {
    if (!open) return;
    const id = setTimeout(() => inputRef.current?.focus(), 0);
    return () => clearTimeout(id);
  }, [open]);

  // Keep the current value selectable even if it's not in the loaded list.
  const all = useMemo(
    () => (value && !options.includes(value) ? [value, ...options] : options),
    [value, options],
  );
  const filtered = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return needle ? all.filter((o) => o.toLowerCase().includes(needle)) : all;
  }, [q, all]);

  return (
    <Select.Root
      value={value || undefined}
      onValueChange={onChange}
      open={open}
      onOpenChange={(o) => {
        setOpen(o);
        if (!o) setQ('');
      }}
    >
      <Select.Trigger
        className={`inline-flex items-center gap-2 rounded-[9px] border border-border-strong bg-surface-2 py-2.25 pl-3.25 pr-3 text-[13.5px] font-semibold text-text outline-none focus:border-accent/60 data-[placeholder]:text-dim ${className}`}
      >
        <span className="truncate">
          <Select.Value placeholder={placeholder} />
        </span>
        <Select.Icon className="shrink-0 text-dim">
          <IconChevronDown size={13} stroke={2.5} />
        </Select.Icon>
      </Select.Trigger>

      <Select.Portal>
        <Select.Content
          position="popper"
          sideOffset={6}
          className="z-50 w-[var(--radix-select-trigger-width)] min-w-60 overflow-hidden rounded-[11px] border border-border-strong bg-[#121216] shadow-pop"
        >
          <div className="flex items-center gap-2 border-b border-border px-3 py-2.5">
            <IconSearch size={14} className="shrink-0 text-dim" />
            <input
              ref={inputRef}
              value={q}
              onChange={(e) => setQ(e.target.value)}
              // Let arrows/enter/escape drive the list; type chars into the box
              // (stop Radix's typeahead from swallowing them).
              onKeyDown={(e) => {
                if (!['ArrowDown', 'ArrowUp', 'Enter', 'Escape'].includes(e.key)) {
                  e.stopPropagation();
                }
              }}
              placeholder={searchPlaceholder}
              className="w-full bg-transparent text-[13px] font-medium text-text outline-none placeholder:text-dim"
            />
          </div>
          <Select.Viewport className="max-h-64 overflow-y-auto p-1.5">
            {filtered.length === 0 ? (
              <div className="px-3 py-4 text-center text-[12.5px] text-dim">-</div>
            ) : (
              filtered.map((o) => (
                <Select.Item
                  key={o}
                  value={o}
                  className="relative flex cursor-pointer select-none items-center rounded-[7px] py-2 pl-3 pr-8 text-[13px] font-medium text-text outline-none data-[highlighted]:bg-white/[.06] data-[state=checked]:text-accent"
                >
                  <Select.ItemText>{o}</Select.ItemText>
                  <Select.ItemIndicator className="absolute right-2.5">
                    <IconCheck size={14} stroke={2.4} />
                  </Select.ItemIndicator>
                </Select.Item>
              ))
            )}
          </Select.Viewport>
        </Select.Content>
      </Select.Portal>
    </Select.Root>
  );
}
