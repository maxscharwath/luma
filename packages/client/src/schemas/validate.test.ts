import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { validate } from './validate';

describe('validate', () => {
  it('returns the exact same reference when the data matches', () => {
    const data = { a: 1, b: 'x' };
    const out = validate(z.object({ a: z.number(), b: z.string() }), data);
    expect(out).toBe(data);
  });

  it('throws (ZodError) when the data does not match the schema', () => {
    expect(() =>
      validate(z.object({ a: z.number() }), { a: 'not-a-number' } as unknown as { a: number }),
    ).toThrow();
  });

  it('validates arrays too', () => {
    const arr = [1, 2, 3];
    expect(validate(z.array(z.number()), arr)).toBe(arr);
    expect(() => validate(z.array(z.number()), [1, 'x'] as unknown as number[])).toThrow();
  });
});
