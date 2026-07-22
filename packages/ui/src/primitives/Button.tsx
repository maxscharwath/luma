// <Button>: the action primitive.
//
// It is a <Focusable>, so the same component is a mouse button in the browser
// and a D-pad target on a TV, with the amber ring and the design's 1.04 press
// scale already wired. Variants are declared once with `sv` rather than
// assembled from conditionals at the call site.

import type { ReactNode } from 'react';
import { Focusable, type FocusableProps } from '../focus/Focusable';
import { sv } from '../system/sv';
import { colors, radius, type as typeRoles } from '../tokens';
import { Icon, type IconName } from './Icon';
import { Txt } from './Text';

const button = sv({
  base: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 9,
    borderRadius: radius.md,
  },
  variants: {
    variant: {
      primary: { backgroundColor: colors.accent },
      glass: {
        backgroundColor: 'rgba(255, 255, 255, 0.1)',
        borderWidth: 1,
        borderColor: colors.borderStrong,
      },
      ghost: { backgroundColor: 'transparent' },
      danger: { backgroundColor: colors.danger },
      /** A bordered toggle: the detail screen's "Ma liste" / "Vu" pills, which
       *  read as pressed rather than as a primary action. */
      outline: {
        backgroundColor: 'rgba(255, 255, 255, 0.12)',
        borderWidth: 1,
        borderColor: 'rgba(255, 255, 255, 0.2)',
      },
    },
    active: {
      true: {},
      false: {},
    },
    size: {
      sm: { paddingVertical: 9, paddingHorizontal: 16 },
      md: { paddingVertical: 14, paddingHorizontal: 28 },
      lg: { paddingVertical: 17, paddingHorizontal: 38 },
      /** The 10-foot primary action (the home hero, a detail screen's Lecture). */
      tv: { paddingVertical: 18, paddingHorizontal: 40 },
    },
    block: {
      true: { alignSelf: 'stretch' },
      false: {},
    },
  },
  compound: [
    {
      when: { variant: 'outline', active: 'true' },
      style: { backgroundColor: colors.accentSoft, borderColor: 'rgba(242, 180, 66, 0.45)' },
    },
  ],
  defaults: { variant: 'primary', size: 'md', block: 'false', active: 'false' },
});

export type ButtonVariant = 'primary' | 'glass' | 'ghost' | 'danger' | 'outline';
export type ButtonSize = 'sm' | 'md' | 'lg' | 'tv';

/** Label metrics per size, matching the design's button scale. */
const LABEL = {
  sm: { fontSize: 13, fontWeight: '600' as const },
  md: { fontSize: 16, fontWeight: '700' as const },
  lg: { fontSize: 19, fontWeight: '700' as const },
  tv: { fontSize: 20, fontWeight: '700' as const },
} satisfies Record<ButtonSize, { fontSize: number; fontWeight: '600' | '700' }>;

const ICON_SIZE = { sm: 16, md: 20, lg: 22, tv: 22 } satisfies Record<ButtonSize, number>;

/** Ink colour per variant: amber fills carry the dark ink, everything else the
 * body text colour. */
const INK = {
  primary: 'accentInk',
  glass: 'text',
  ghost: 'text',
  danger: 'text',
  outline: 'text',
} as const;

export interface ButtonProps
  extends Omit<FocusableProps, 'children' | 'style' | 'focusScale' | 'label' | 'ring'> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Stretch to the width of the parent. */
  block?: boolean;
  /** Pressed state of a toggle. Only the `outline` variant paints it. */
  active?: boolean;
  /** Leading glyph. */
  icon?: IconName;
  /** Trailing glyph (a chevron on a settings row, for instance). */
  iconRight?: IconName;
  /** Text label. It is also the accessibility name. Pass `children` instead for
   *  anything richer than a string. */
  label?: string;
  children?: ReactNode;
  style?: FocusableProps['style'];
  /** Focus scale. Defaults to the design's 1.04 for the primary action. */
  focusScale?: number;
}

export function Button({
  variant = 'primary',
  size = 'md',
  block = false,
  active = false,
  icon,
  iconRight,
  label,
  children,
  style,
  disabled = false,
  focusScale = 1.04,
  ...focusProps
}: Readonly<ButtonProps>) {
  // An active toggle tints its glyph and label amber along with its fill.
  const ink = variant === 'outline' && active ? 'accent' : INK[variant];
  const glyph = ICON_SIZE[size];
  return (
    <Focusable
      {...focusProps}
      disabled={disabled}
      focusScale={focusScale}
      label={label}
      style={button(
        { variant, size, block: block ? 'true' : 'false', active: active ? 'true' : 'false' },
        disabled ? DISABLED : null,
        style,
      )}
    >
      {icon ? <Icon name={icon} size={glyph} color={ink} /> : null}
      {label === undefined ? null : (
        <Txt color={ink} style={{ ...typeRoles.label, ...LABEL[size] }}>
          {label}
        </Txt>
      )}
      {children}
      {iconRight ? <Icon name={iconRight} size={glyph} color={ink} /> : null}
    </Focusable>
  );
}

const DISABLED = { opacity: 0.5 } as const;
