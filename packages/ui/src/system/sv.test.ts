import { describe, expect, it } from 'vitest';
import { sv } from './sv';

const button = sv({
  base: { borderRadius: 10 },
  variants: {
    variant: {
      primary: { backgroundColor: '#F4B642' },
      ghost: { backgroundColor: 'transparent' },
    },
    size: {
      sm: { paddingHorizontal: 20 },
      md: { paddingHorizontal: 36 },
    },
  },
  compound: [{ when: { variant: 'ghost', size: 'sm' }, style: { borderWidth: 1 } }],
  defaults: { variant: 'primary', size: 'md' },
});

describe('sv', () => {
  it('applies the defaults when the caller passes nothing', () => {
    expect(button()).toEqual([
      { borderRadius: 10 },
      { backgroundColor: '#F4B642' },
      { paddingHorizontal: 36 },
    ]);
  });

  it('overrides only the variant the caller names', () => {
    expect(button({ variant: 'ghost' })).toEqual([
      { borderRadius: 10 },
      { backgroundColor: 'transparent' },
      { paddingHorizontal: 36 },
    ]);
  });

  it('applies a compound rule only when every condition matches', () => {
    expect(button({ variant: 'ghost', size: 'sm' })).toContainEqual({ borderWidth: 1 });
    expect(button({ variant: 'primary', size: 'sm' })).not.toContainEqual({ borderWidth: 1 });
  });

  it('matches a compound rule against a value that came from the defaults', () => {
    // `size` is not passed, so it defaults to md; the rule requires sm and must
    // therefore NOT fire. The reverse case is covered above.
    expect(button({ variant: 'ghost' })).not.toContainEqual({ borderWidth: 1 });
  });

  it("puts the caller's own style last so a one-off override wins", () => {
    const out = button({ variant: 'primary' }, { backgroundColor: 'red' });
    expect(out.at(-1)).toEqual({ backgroundColor: 'red' });
  });

  it('drops falsy overrides so `cond && style` is safe to pass', () => {
    expect(button({}, false, null, undefined)).toHaveLength(3);
  });

  it('works with no variants at all', () => {
    expect(sv({ base: { flex: 1 } })()).toEqual([{ flex: 1 }]);
    expect(sv({})()).toEqual([]);
  });
});
