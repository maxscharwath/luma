// The two remote-driven on-screen keyboards: a full layout for server URLs and a
// dedicated search layout (matching the KROMA design). Everything interactive
// carries `data-focus` so the spatial focus nav (useFocusNav) reaches it and OK
// activates via the native click. Letter ordering follows the device's persisted
// layout preference (ABC / AZERTY / QWERTY / QWERTZ, see keyboardLayoutPref).

import { IconBackspace, IconSpace, IconX } from '@tabler/icons-react';
import { type ReactNode, useEffect, useMemo, useRef, useState } from 'react';
import { getKeyboardLayoutPref, type KeyboardLayoutPref } from '#tv/app/keyboardLayoutPref';
import { useEnv } from '#tv/app/providers/env';
import { LAYOUT_LETTER_ROWS, urlRows } from './keyboardLayouts';

// ----- physical-keyboard bridge -------------------------------------------------

/** On devices with a hardware keyboard (useEnv().physicalKeyboard never a real
 * TV shell), let the user type straight into the value while the on-screen
 * keyboard is up, whatever element holds the spatial focus. The real `<input>`
 * (TvTextEntry) handles its own typing, so events targeting it are skipped;
 * printable keys and Backspace are consumed here (Space intentionally types a
 * space instead of activating the focused key typing wins on keyboard devices).
 * D-pad / Enter / Escape stay with the focus nav. */
function usePhysicalTyping(value: string, onChange: (next: string) => void) {
  const { physicalKeyboard } = useEnv();
  const stateRef = useRef({ value, onChange });
  stateRef.current = { value, onChange };
  useEffect(() => {
    if (!physicalKeyboard) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return;
      const t = e.target;
      if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) return;
      const s = stateRef.current;
      if (e.key === 'Backspace') {
        e.preventDefault();
        s.onChange(s.value.slice(0, -1));
        return;
      }
      if (e.key.length === 1) {
        e.preventDefault();
        s.onChange(s.value + e.key);
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [physicalKeyboard]);
}

// ----- layout preference ------------------------------------------------------

/** The device's layout preference mapped through `derive`, computed ONCE per
 * mount. Both keyboards re-render on EVERY keystroke, and `localStorage.getItem`
 * is a blocking cross-process hop on the old TV webviews, so neither the read
 * nor the row building it feeds may sit in the render body. Changing the layout
 * still lands: its picker is a screen of its own (the profile menu), so the
 * keyboard is unmounted while it happens and the next mount reads the new value.
 * `derive` must be a module-level (stable) function. */
function useLayout<T>(derive: (layout: KeyboardLayoutPref) => T): T {
  const [layout] = useState(getKeyboardLayoutPref);
  return useMemo(() => derive(layout), [derive, layout]);
}

// ----- on-screen keyboard -----------------------------------------------------

const KB_KEY =
  'flex cursor-pointer items-center justify-center rounded-xl bg-[rgba(255,255,255,0.05)] font-sans font-bold text-text transition-transform focus:scale-[1.08] focus:bg-[rgba(244,182,66,0.18)] focus:text-accent';

/** A remote-driven on-screen keyboard. The caller owns the text value; each key
 * mutates it through `onChange`, and the special keys (space / delete / clear /
 * submit / close) call the matching handler. `layout` swaps between the
 * server-URL keyboard ({@link UrlKeyboard}) and the search keyboard (which has
 * its own dedicated design, {@link SearchKeyboard}). */
export function OnScreenKeyboard({
  value,
  onChange,
  onSubmit,
  onClose,
  layout = 'search',
  submitLabel,
}: Readonly<{
  value: string;
  onChange: (next: string) => void;
  onSubmit?: () => void;
  onClose?: () => void;
  layout?: 'url' | 'search';
  submitLabel?: string;
}>) {
  usePhysicalTyping(value, onChange);

  return layout === 'search' ? (
    <SearchKeyboard value={value} onChange={onChange} onClose={onClose} />
  ) : (
    <UrlKeyboard value={value} onChange={onChange} onSubmit={onSubmit} submitLabel={submitLabel} />
  );
}

/** The server-URL keyboard: a digit row, the preferred layout's letters as rows
 * of ten lowercase keys with the URL specials appended, then clear / "." / the
 * optional submit button. */
function UrlKeyboard({
  value,
  onChange,
  onSubmit,
  submitLabel,
}: Readonly<{
  value: string;
  onChange: (next: string) => void;
  onSubmit?: () => void;
  submitLabel?: string;
}>) {
  const rows = useLayout(urlRows);
  const press = (k: string) => {
    if (k === '⌫') onChange(value.slice(0, -1));
    else onChange(value + k);
  };
  return (
    <div className="flex flex-col gap-3">
      {rows.map((row) => (
        <div key={row.join('')} className="flex gap-3">
          {row.map((k) => (
            <button
              key={k}
              data-focus=""
              type="button"
              onClick={() => press(k)}
              className={`${KB_KEY} h-13 flex-1 text-[20px]`}
            >
              {k}
            </button>
          ))}
        </div>
      ))}
      <div className="flex gap-3">
        <button
          data-focus=""
          type="button"
          onClick={() => onChange('')}
          className={`${KB_KEY} h-13 flex-2 text-[16px]`}
        >
          ⌧
        </button>
        <button
          data-focus=""
          type="button"
          onClick={() => onChange(`${value}.`)}
          className={`${KB_KEY} h-13 flex-1 text-[20px]`}
        >
          .
        </button>
        {onSubmit ? (
          <button
            data-focus=""
            type="button"
            onClick={onSubmit}
            className="flex h-13 flex-3 cursor-pointer items-center justify-center rounded-xl bg-accent font-sans text-[17px] font-bold text-accent-ink transition-transform focus:scale-[1.06]"
          >
            {submitLabel}
          </button>
        ) : null}
      </div>
    </div>
  );
}

// ----- search keyboard --------------------------------------------------------

const SEARCH_DIGITS = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];

/** Everything the search keyboard's look derives from a layout's letter rows.
 * Typewriter layouts run ten keys per row; in the 520px column that only reads
 * as a keyboard with uniform fixed near-square keys and centered rows (the
 * natural stagger). Stretchy flex-1 keys would give every row a different key
 * width. The ABC grid keeps the original roomy 6-column design. Built once per
 * mount (see {@link useLayout}), never per keystroke. */
function searchLook(layout: KeyboardLayoutPref) {
  const letterRows = LAYOUT_LETTER_ROWS[layout];
  const wide = letterRows.some((r) => r.length > 6);
  return {
    letterRows,
    lastRow: letterRows.at(-1) ?? [],
    wide,
    cell: `flex cursor-pointer items-center justify-center rounded-xl bg-[rgba(255,255,255,0.05)] font-sans font-bold text-text transition-transform focus:scale-[1.08] focus:bg-accent focus:text-accent-ink ${
      wide ? 'h-12 w-11 flex-none text-[19px]' : 'h-14 flex-1 text-[22px]'
    }`,
    rowCls: wide ? 'flex justify-center gap-2' : 'flex gap-3',
    iconSize: wide ? 22 : 26,
  };
}

/** The search on-screen keyboard, matching the KROMA design: a 1-0 digit row,
 * the uppercase alphabet in the preferred layout's rows, and a final row pairing
 * the layout's trailing letters with space, backspace and a close key. Letters
 * insert lowercase (search is case-insensitive); the focused key fills solid
 * amber for a strong 10-foot cue. */
function SearchKeyboard({
  value,
  onChange,
  onClose,
}: Readonly<{ value: string; onChange: (next: string) => void; onClose?: () => void }>) {
  const { letterRows, lastRow, wide, cell, rowCls, iconSize } = useLayout(searchLook);
  // A render helper (not a nested component) so the <button> element type stays
  // stable across the per-keypress re-render and focus is never lost.
  const key = (id: string, label: ReactNode, onPress: () => void) => (
    <button key={id} data-focus="" type="button" onClick={onPress} className={cell}>
      {label}
    </button>
  );
  const letter = (l: string) => key(l, l, () => onChange(value + l.toLowerCase()));
  return (
    <div className={`flex flex-col ${wide ? 'gap-2' : 'gap-3'}`}>
      <div className={rowCls}>{SEARCH_DIGITS.map((d) => key(d, d, () => onChange(value + d)))}</div>
      {letterRows.slice(0, -1).map((row) => (
        <div key={row.join('')} className={rowCls}>
          {row.map(letter)}
        </div>
      ))}
      <div className={rowCls}>
        {lastRow.map(letter)}
        {key('space', <IconSpace size={wide ? 24 : 28} stroke={1.8} />, () =>
          onChange(`${value} `),
        )}
        {key('delete', <IconBackspace size={iconSize} stroke={1.8} />, () =>
          onChange(value.slice(0, -1)),
        )}
        {key('close', <IconX size={wide ? 20 : 24} stroke={2} />, () => onClose?.())}
      </div>
    </div>
  );
}
