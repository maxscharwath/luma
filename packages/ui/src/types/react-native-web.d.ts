// Props react-native-web adds to the React Native surface. They are no-ops on
// Apple TV and Android TV (React Native ignores unknown props), which is exactly
// what we want: one component can declare its web affordances inline instead of
// being split into two files for the sake of one attribute.

import 'react-native';

declare module 'react-native' {
  interface ViewProps {
    /** Rendered as `data-*` attributes. The spatial navigator finds focusables
     *  and focus scopes this way. */
    dataSet?: Record<string, string | number | undefined> | undefined;
    /** Tab order. `-1` removes a disabled control from keyboard navigation. */
    tabIndex?: number | undefined;
  }
}
