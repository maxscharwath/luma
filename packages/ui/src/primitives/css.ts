// CSS features React Native supports under an `experimental_` prefix (native)
// but that react-native-web exposes under their plain CSS name. See css.web.ts.
//
// Keeping the prefix difference behind these three helpers is what lets every
// gradient in the app stay a single CSS string in a single source file, instead
// of a CSS value on the web and a <LinearGradient> component on native.

import type { ViewStyle } from 'react-native';

/** A CSS `background-image` value, e.g. `linear-gradient(158deg, #a 0%, #b 72%)`. */
export function gradient(css: string): ViewStyle {
  return { experimental_backgroundImage: css };
}

/** A CSS `background-position` value, e.g. `50% 28%`. */
export function bgPosition(value: string): ViewStyle {
  return { experimental_backgroundPosition: value };
}

/** A CSS `background-size` value, e.g. `cover`. */
export function bgSize(value: string): ViewStyle {
  return { experimental_backgroundSize: value };
}
