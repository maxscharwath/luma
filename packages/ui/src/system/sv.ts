// `sv` = style variants. What `cva` is to Tailwind class strings, this is to
// React Native styles.
//
// Why it exists: React Native has no className, so the honest way to express "a
// primary button, large" is a lookup from props to style objects. Hand-rolling
// that per component produces the ternary soup this kit is meant to eliminate:
//
//   style={[s.base, variant === 'primary' && s.primary, size === 'lg' && s.lg]}
//
// With `sv` the component declares its design once and reads as a shadcn
// component does:
//
//   const button = sv({
//     base: { flexDirection: 'row', alignItems: 'center', borderRadius: radius.md },
//     variants: {
//       variant: { primary: { backgroundColor: colors.accent }, ghost: {} },
//       size: { md: { paddingHorizontal: 36 }, sm: { paddingHorizontal: 20 } },
//     },
//     compound: [{ when: { variant: 'primary', size: 'sm' }, style: { borderRadius: 8 } }],
//     defaults: { variant: 'primary', size: 'md' },
//   });
//
//   <View style={button({ variant, size }, style)} />
//
// The caller's own `style` is always merged LAST, so a one-off override wins,
// exactly like passing `className` to a shadcn component.

import type { StyleProp, ViewStyle } from 'react-native';

// ViewStyle is the widest of React Native's style shapes: TextStyle and
// ImageStyle both extend it, so one type covers every component's variants.
type Style = ViewStyle;

/** A variant group: the prop name maps to its options' styles. */
type VariantGroups = Record<string, Record<string, Style>>;

/** The props a compiled `sv` accepts: one optional key per variant group,
 * typed to that group's option names. */
export type VariantProps<V extends VariantGroups> = {
  [K in keyof V]?: keyof V[K];
};

export interface CompoundVariant<V extends VariantGroups> {
  /** All of these must match for `style` to apply. */
  when: VariantProps<V>;
  style: Style;
}

export interface SvConfig<V extends VariantGroups> {
  /** Always applied, first. */
  base?: Style;
  variants?: V;
  /** Styles that only apply to a COMBINATION of variants. */
  compound?: readonly CompoundVariant<V>[];
  /** Used when the caller leaves a variant prop undefined. */
  defaults?: VariantProps<V>;
}

/**
 * The compiled variant function. Extra `overrides` are appended last.
 *
 * It returns a flat `ViewStyle[]` rather than the wider `StyleProp<ViewStyle>`:
 * an array is what React Native wants anyway, and the concrete type keeps the
 * result inspectable (in tests, and when composing one variant set into another)
 * instead of collapsing to a union you have to narrow first.
 */
export type SvFn<V extends VariantGroups> = (
  props?: VariantProps<V>,
  ...overrides: StyleProp<ViewStyle>[]
) => ViewStyle[];

export function sv<V extends VariantGroups>(config: SvConfig<V>): SvFn<V> {
  const { base, variants, compound, defaults } = config;
  const groups = variants ? (Object.keys(variants) as (keyof V)[]) : [];

  return (props, ...overrides) => {
    const out: ViewStyle[] = [];
    if (base) out.push(base);

    // Resolve each group once, keeping the picks for the compound pass so a
    // compound rule sees the SAME resolved value the caller will render with
    // (including the defaults it never passed).
    const picked: VariantProps<V> = {};
    for (const group of groups) {
      const value = props?.[group] ?? defaults?.[group];
      if (value === undefined) continue;
      picked[group] = value;
      const style = variants?.[group]?.[value as string];
      if (style) out.push(style);
    }

    for (const rule of compound ?? []) {
      const matches = Object.entries(rule.when).every(
        ([group, value]) => picked[group as keyof V] === value,
      );
      if (matches) out.push(rule.style);
    }

    // React Native flattens nested style arrays itself, so a caller's own
    // `style` can be pushed through whatever shape it arrives in.
    for (const override of overrides) {
      if (override) out.push(override as ViewStyle);
    }
    return out;
  };
}
