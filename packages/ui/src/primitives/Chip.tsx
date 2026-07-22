// <Chip>: the pill filter / selector (language codes, audio formats, genres,
// recent searches). Focusable, so the same component is a click target in the
// browser and a D-pad stop on a TV.

import type { ReactNode } from 'react';
import { Focusable, type FocusableProps } from '../focus/Focusable';
import { sv } from '../system/sv';
import { colors, fonts, radius } from '../tokens';
import { Txt } from './Text';

const chip = sv({
  base: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    paddingVertical: 6,
    paddingHorizontal: 14,
    borderRadius: radius.pill,
    borderWidth: 1,
    borderColor: colors.border,
  },
  variants: {
    active: {
      true: { backgroundColor: colors.accent },
      false: { backgroundColor: 'rgba(255, 255, 255, 0.07)' },
    },
    size: {
      sm: {},
      /** The 10-foot size: bigger tap area and type for a 3 m viewing distance. */
      tv: { paddingVertical: 10, paddingHorizontal: 22 },
    },
  },
  defaults: { active: 'false', size: 'sm' },
});

const LABEL = {
  sm: { fontFamily: fonts.ui, fontWeight: '600' as const, fontSize: 13 },
  tv: { fontFamily: fonts.ui, fontWeight: '600' as const, fontSize: 18 },
};

export interface ChipProps extends Omit<FocusableProps, 'children' | 'style' | 'label'> {
  active?: boolean;
  size?: 'sm' | 'tv';
  label?: string;
  children?: ReactNode;
  style?: FocusableProps['style'];
}

export function Chip({
  active = false,
  size = 'sm',
  label,
  children,
  style,
  ...focusProps
}: Readonly<ChipProps>) {
  return (
    <Focusable
      {...focusProps}
      label={label}
      style={chip({ active: active ? 'true' : 'false', size }, style)}
    >
      {label === undefined ? null : (
        <Txt style={{ ...LABEL[size], color: active ? colors.accentInk : colors.text }}>
          {label}
        </Txt>
      )}
      {children}
    </Focusable>
  );
}
