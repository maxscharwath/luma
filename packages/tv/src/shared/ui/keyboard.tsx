// The two remote-driven on-screen keyboards: a full layout for server URLs and a
// dedicated search layout (matching the KROMA design). Everything interactive
// carries `data-focus` so the spatial focus nav (useFocusNav) reaches it and OK
// activates via the native click.

import { IconBackspace, IconSpace, IconX } from '@tabler/icons-react';
import type { ReactNode } from 'react';

// ----- on-screen keyboard -----------------------------------------------------

const KB_KEY =
  'flex cursor-pointer items-center justify-center rounded-xl bg-[rgba(255,255,255,0.05)] font-sans font-bold text-text transition-transform focus:scale-[1.08] focus:bg-[rgba(244,182,66,0.18)] focus:text-accent';

/** A remote-driven on-screen keyboard. The caller owns the text value; each key
 * mutates it through `onChange`, and the special keys (space / delete / clear /
 * submit / close) call the matching handler. `layout` swaps between the
 * server-URL keyboard and the search keyboard (which has its own dedicated
 * layout, {@link SearchKeyboard}). */
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
  if (layout === 'search')
    return <SearchKeyboard value={value} onChange={onChange} onClose={onClose} />;

  const press = (k: string) => {
    if (k === '⌫') onChange(value.slice(0, -1));
    else onChange(value + k);
  };
  return (
    <div className="flex flex-col gap-3">
      {URL_ROWS.map((row) => (
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
          className={`${KB_KEY} h-13 flex-[2] text-[16px]`}
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
            className="flex h-13 flex-[3] cursor-pointer items-center justify-center rounded-xl bg-accent font-sans text-[17px] font-bold text-accent-ink transition-transform focus:scale-[1.06]"
          >
            {submitLabel}
          </button>
        ) : null}
      </div>
    </div>
  );
}

const URL_ROWS = [
  ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
  ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'],
  ['k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't'],
  ['u', 'v', 'w', 'x', 'y', 'z', '-', ':', '/', '⌫'],
];

// ----- search keyboard --------------------------------------------------------

const SEARCH_DIGITS = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
const SEARCH_LETTER_ROWS = [
  ['A', 'B', 'C', 'D', 'E', 'F'],
  ['G', 'H', 'I', 'J', 'K', 'L'],
  ['M', 'N', 'O', 'P', 'Q', 'R'],
  ['S', 'T', 'U', 'V', 'W', 'X'],
];

/** The search on-screen keyboard, matching the KROMA design: a 1–0 digit row, the
 * uppercase alphabet in rows of six, and a final row pairing Y / Z with space,
 * backspace and a close key. Letters insert lowercase (search is
 * case-insensitive); the focused key fills solid amber for a strong 10-foot cue. */
function SearchKeyboard({
  value,
  onChange,
  onClose,
}: Readonly<{ value: string; onChange: (next: string) => void; onClose?: () => void }>) {
  const cell =
    'flex h-14 flex-1 cursor-pointer items-center justify-center rounded-xl bg-[rgba(255,255,255,0.05)] font-sans text-[22px] font-bold text-text transition-transform focus:scale-[1.08] focus:bg-accent focus:text-accent-ink';
  // A render helper (not a nested component) so the <button> element type stays
  // stable across the per-keypress re-render and focus is never lost.
  const key = (id: string, label: ReactNode, onPress: () => void) => (
    <button key={id} data-focus="" type="button" onClick={onPress} className={cell}>
      {label}
    </button>
  );
  return (
    <div className="flex flex-col gap-3">
      <div className="flex gap-3">
        {SEARCH_DIGITS.map((d) => key(d, d, () => onChange(value + d)))}
      </div>
      {SEARCH_LETTER_ROWS.map((row) => (
        <div key={row.join('')} className="flex gap-3">
          {row.map((l) => key(l, l, () => onChange(value + l.toLowerCase())))}
        </div>
      ))}
      <div className="flex gap-3">
        {key('Y', 'Y', () => onChange(`${value}y`))}
        {key('Z', 'Z', () => onChange(`${value}z`))}
        {key('space', <IconSpace size={28} stroke={1.8} />, () => onChange(`${value} `))}
        {key('delete', <IconBackspace size={26} stroke={1.8} />, () =>
          onChange(value.slice(0, -1)),
        )}
        {key('close', <IconX size={24} stroke={2} />, () => onClose?.())}
      </div>
    </div>
  );
}
