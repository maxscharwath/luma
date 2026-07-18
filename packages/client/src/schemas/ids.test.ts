import { describe, expect, it } from 'vitest';
import { ItemId, ShowId, UserId } from './ids';

describe('branded id schemas', () => {
  it('.of parses a raw string and returns it unchanged (brand is compile-time)', () => {
    expect(UserId.of('u_123')).toBe('u_123');
    expect(ItemId.of('i_1')).toBe('i_1');
    expect(ShowId.of('s_9')).toBe('s_9');
  });

  it('.parse behaves like .of (the ergonomic alias)', () => {
    expect(ItemId.parse('i_2')).toBe('i_2');
  });

  it('rejects non-string input', () => {
    expect(() => UserId.of(42 as unknown as string)).toThrow();
    expect(UserId.safeParse(42).success).toBe(false);
    expect(ItemId.safeParse(null).success).toBe(false);
  });

  it('accepts any string (structural validation is just a string check)', () => {
    expect(UserId.of('')).toBe('');
    expect(ItemId.of('anything at all')).toBe('anything at all');
  });
});
