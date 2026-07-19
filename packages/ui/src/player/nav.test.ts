import { describe, expect, it } from 'vitest';
import { controlOrder } from './nav';
import { TV_FLAGS, WEB_FLAGS } from './types';

describe('controlOrder', () => {
  it('always begins with the transport trio', () => {
    const row = controlOrder(TV_FLAGS, false);
    expect(row.slice(0, 3)).toEqual(['rewind', 'play', 'forward']);
  });

  it('omits every flagged control on TV (no volume / pip / fullscreen stops)', () => {
    expect(controlOrder(TV_FLAGS, false)).toEqual([
      'rewind',
      'play',
      'forward',
      'subtitles',
      'audio',
      'settings',
    ]);
  });

  it('includes volume / pip / fullscreen on web, in visual order', () => {
    expect(controlOrder(WEB_FLAGS, false)).toEqual([
      'rewind',
      'play',
      'forward',
      'volume',
      'subtitles',
      'audio',
      'settings',
      'pip',
      'fullscreen',
    ]);
  });

  it('inserts "next" right after the transport trio when there is a next episode', () => {
    const row = controlOrder(TV_FLAGS, true);
    expect(row[3]).toBe('next');
    expect(controlOrder(WEB_FLAGS, true).indexOf('next')).toBe(3);
  });

  it('honors individual flags independently', () => {
    const row = controlOrder({ volume: true, pip: false, fullscreen: true, pointer: true }, false);
    expect(row).toContain('volume');
    expect(row).not.toContain('pip');
    expect(row).toContain('fullscreen');
  });
});
