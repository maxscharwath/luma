// Letter-row data for the on-screen keyboards, per user-selectable layout
// (see keyboardLayoutPref). Rows are uppercase; the search keyboard renders
// them as-is (inserting lowercase), the URL keyboard flattens them into rows
// of ten lowercase keys.

import type { KeyboardLayoutPref } from '#tv/app/keyboardLayoutPref';

/** Uppercase letter rows per layout. The LAST row is deliberately short: the
 * search keyboard appends space / backspace / close to it. */
export const LAYOUT_LETTER_ROWS: Record<KeyboardLayoutPref, readonly (readonly string[])[]> = {
  abc: [
    ['A', 'B', 'C', 'D', 'E', 'F'],
    ['G', 'H', 'I', 'J', 'K', 'L'],
    ['M', 'N', 'O', 'P', 'Q', 'R'],
    ['S', 'T', 'U', 'V', 'W', 'X'],
    ['Y', 'Z'],
  ],
  azerty: [
    ['A', 'Z', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P'],
    ['Q', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L', 'M'],
    ['W', 'X', 'C', 'V', 'B', 'N'],
  ],
  qwerty: [
    ['Q', 'W', 'E', 'R', 'T', 'Y', 'U', 'I', 'O', 'P'],
    ['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L'],
    ['Z', 'X', 'C', 'V', 'B', 'N', 'M'],
  ],
  qwertz: [
    ['Q', 'W', 'E', 'R', 'T', 'Z', 'U', 'I', 'O', 'P'],
    ['A', 'S', 'D', 'F', 'G', 'H', 'J', 'K', 'L'],
    ['Y', 'X', 'C', 'V', 'B', 'N', 'M'],
  ],
};

/** URL-keyboard rows for a layout: the digits row, then the layout's letters
 * (lowercase) flattened and chunked into rows of ten, with the URL specials
 * appended to the tail 26 letters + 4 specials = exactly three rows of ten. */
export function urlRows(layout: KeyboardLayoutPref): string[][] {
  const keys = [
    ...LAYOUT_LETTER_ROWS[layout].flat().map((l) => l.toLowerCase()),
    '-',
    ':',
    '/',
    '⌫',
  ];
  const rows: string[][] = [['1', '2', '3', '4', '5', '6', '7', '8', '9', '0']];
  for (let i = 0; i < keys.length; i += 10) rows.push(keys.slice(i, i + 10));
  return rows;
}
