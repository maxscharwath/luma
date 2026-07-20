// The editable text region inside a search / server-URL field.
//
// On a shell with a physical keyboard (desktop) it is a real, focusable,
// clickable <input> the user types into; on a TV (which drives an on-screen
// keyboard) it is a non-focusable display of the current value plus a blinking
// cursor, so nothing invites a click that would summon the platform IME. The
// caller owns the surrounding box + icons the shared text/placeholder classes
// keep the two modes pixel-matched.

import { type KeyboardEvent as ReactKeyboardEvent, useEffect, useRef } from 'react';
import { useEnv } from '#tv/app/providers/env';

export interface TvTextEntryProps {
  value: string;
  onChange: (next: string) => void;
  /** Fired on Enter (desktop input only); the on-screen keyboard has its own submit key. */
  onSubmit?: () => void;
  placeholder?: string;
  /** Text styling shared by the desktop <input> and the TV display span. */
  textClassName?: string;
  /** Placeholder colour for the TV display span (desktop uses `::placeholder`). */
  placeholderClassName?: string;
  /** Blinking-cursor classes (TV only desktop shows the native amber caret). */
  cursorClassName?: string;
  inputMode?: 'text' | 'url' | 'search';
  ariaLabel?: string;
  /** Focus the input on mount (desktop) so a keyboard user can type immediately. */
  autoFocus?: boolean;
}

export function TvTextEntry({
  value,
  onChange,
  onSubmit,
  placeholder,
  textClassName = '',
  placeholderClassName = 'text-[rgba(244,243,240,0.3)]',
  cursorClassName,
  inputMode = 'text',
  ariaLabel,
  autoFocus = true,
}: Readonly<TvTextEntryProps>) {
  const { physicalKeyboard } = useEnv();
  const ref = useRef<HTMLInputElement>(null);

  // Focus on mount so typing works at once (the screen's first `[data-focus]` is
  // often a Back button, so useFocusNav wouldn't land here). Child effects run
  // before the parent's useFocusNav, which then leaves this focused input alone.
  useEffect(() => {
    if (physicalKeyboard && autoFocus) ref.current?.focus({ preventScroll: true });
  }, [physicalKeyboard, autoFocus]);

  if (physicalKeyboard) {
    return (
      <input
        ref={ref}
        data-focus=""
        type="text"
        inputMode={inputMode}
        aria-label={ariaLabel}
        placeholder={placeholder}
        value={value}
        spellCheck={false}
        autoComplete="off"
        autoCapitalize="off"
        autoCorrect="off"
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e: ReactKeyboardEvent<HTMLInputElement>) => {
          if (e.key === 'Enter' && onSubmit) onSubmit();
        }}
        // focus:shadow-none suppresses the global [data-focus]:focus amber ring:
        // the surrounding InputGroup shows the focus state instead (shadcn-style,
        // a calm focus-within accent border on the field).
        className={`min-w-0 border-none bg-transparent p-0 caret-accent outline-none placeholder:text-[rgba(244,243,240,0.35)] focus:shadow-none ${textClassName}`}
      />
    );
  }

  return (
    <span className={textClassName}>
      {value || (placeholder ? <span className={placeholderClassName}>{placeholder}</span> : null)}
      {cursorClassName ? <span className={cursorClassName} /> : null}
    </span>
  );
}
