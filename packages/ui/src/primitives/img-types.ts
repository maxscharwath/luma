// The shared <Img> contract. Both renderers (Img.tsx native, Img.web.tsx)
// implement exactly this, so callers never branch on platform.

import type { StyleProp, ViewStyle } from 'react-native';

export interface ImgProps {
  /** Already-sized artwork URL. This component never rewrites it. */
  src: string | null;
  /** Accessibility text. Empty (the default) marks the artwork decorative. */
  alt?: string;
  /** Fade duration in ms, for both the load-in and the cross-fade on `src` change. */
  duration?: number;
  /** How the art fills its box. Default `cover`. */
  fit?: 'cover' | 'contain';
  /** CSS object-position, e.g. `'50% 28%'` (heroes favour the upper third).
   *  Only has a visible effect when `fit` is `cover` AND the art's aspect ratio
   *  differs from the box's, which is why rail tiles can leave it at the default. */
  position?: string;
  /** CSS background painted behind the art: the instant-visible fallback fill
   *  (usually the deterministic genre gradient) shown while loading and on error. */
  background?: string;
  /** Corner radius; the container clips the art to it. */
  radius?: number;
  /** Stretch to fill a positioned parent (absolute, inset 0). */
  fill?: boolean;
  style?: StyleProp<ViewStyle>;
  /** Mark this the above-the-fold LCP art: load it eagerly at high priority
   *  instead of lazily. Use on at most one image per screen. */
  priority?: boolean;
  onLoad?: () => void;
  onError?: () => void;
}

/** Fade default, matching the pre-uikit <Image>. */
export const IMG_FADE_MS = 400;
