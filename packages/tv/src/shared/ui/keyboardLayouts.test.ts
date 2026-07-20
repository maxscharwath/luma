import { describe, expect, it } from 'vitest';
import { ALL_KEYBOARD_LAYOUTS } from '#tv/app/keyboardLayoutPref';
import { LAYOUT_LETTER_ROWS, urlRows } from './keyboardLayouts';

const ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('').sort().join('');

describe('LAYOUT_LETTER_ROWS', () => {
  it('covers every layout preference', () => {
    for (const l of ALL_KEYBOARD_LAYOUTS) {
      expect(LAYOUT_LETTER_ROWS[l]).toBeDefined();
    }
  });

  it('contains each of the 26 letters exactly once per layout', () => {
    for (const l of ALL_KEYBOARD_LAYOUTS) {
      const letters = LAYOUT_LETTER_ROWS[l].flat();
      expect(letters).toHaveLength(26);
      expect([...letters].sort().join('')).toBe(ALPHABET);
    }
  });
});

describe('urlRows', () => {
  it('always yields a digits row plus three rows of ten', () => {
    for (const l of ALL_KEYBOARD_LAYOUTS) {
      const rows = urlRows(l);
      expect(rows).toHaveLength(4);
      for (const row of rows) expect(row).toHaveLength(10);
    }
  });

  it('keeps the historical ABC url grid (lowercase letters then specials)', () => {
    expect(urlRows('abc')).toEqual([
      ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'],
      ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j'],
      ['k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't'],
      ['u', 'v', 'w', 'x', 'y', 'z', '-', ':', '/', '⌫'],
    ]);
  });

  it('orders AZERTY letters in typewriter rows', () => {
    expect(urlRows('azerty')[1]).toEqual(['a', 'z', 'e', 'r', 't', 'y', 'u', 'i', 'o', 'p']);
  });
});
