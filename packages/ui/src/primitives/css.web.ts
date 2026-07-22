// Web half of css.ts: react-native-web passes these straight through to CSS
// under their standard names, with no `experimental_` prefix.

import type { ViewStyle } from 'react-native';

export function gradient(css: string): ViewStyle {
  return { backgroundImage: css } as ViewStyle;
}

export function bgPosition(value: string): ViewStyle {
  return { backgroundPosition: value } as ViewStyle;
}

export function bgSize(value: string): ViewStyle {
  return { backgroundSize: value } as ViewStyle;
}
