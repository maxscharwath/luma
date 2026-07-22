// <Img> on the browser targets (Tizen, webOS, Android TV WebView, desktop).
//
// A real <img> element, NOT react-native-web's Image: RNW renders artwork as a
// div's `background-image`, which forfeits `loading="lazy"`, `fetchpriority` and
// `object-position`. A 1000-poster browse grid on a TV needs all three, so this
// renderer drops to the DOM for the leaf element only. Everything above it (the
// container, the sizing, the gradient fallback) is the same tokens and the same
// layout as the native renderer.

import type { CSSProperties } from 'react';
import { View } from 'react-native';
import { absoluteFill } from '../tokens';
import { useCrossFade } from './crossfade';
import { gradient } from './css';
import { IMG_FADE_MS, type ImgProps } from './img-types';

/* Fill the parent using the four longhands, not the `inset` shorthand, which
   old webOS Chromium 53 does not know and would drop from an inline style. */
const FILL: CSSProperties = {
  position: 'absolute',
  top: 0,
  right: 0,
  bottom: 0,
  left: 0,
  width: '100%',
  height: '100%',
};

export function Img({
  src,
  alt = '',
  duration = IMG_FADE_MS,
  fit = 'cover',
  position = '50% 50%',
  background,
  radius,
  fill = false,
  style,
  priority = false,
  onLoad,
  onError,
}: Readonly<ImgProps>) {
  const { loaded, errored, under, markLoaded, markErrored } = useCrossFade(src, duration);
  const layer = { ...FILL, objectFit: fit, objectPosition: position };

  return (
    <View
      style={[
        fill ? absoluteFill : null,
        { overflow: 'hidden' },
        radius === undefined ? null : { borderRadius: radius },
        background === undefined ? null : gradient(background),
        style,
      ]}
    >
      {under && under !== src ? (
        <img key="under" src={under} alt="" aria-hidden draggable={false} style={layer} />
      ) : null}

      {src && !errored ? (
        <img
          key={src}
          src={src}
          alt={alt}
          // Cached art can already be `complete` before React attaches onLoad,
          // so the event never fires: check the element the moment it mounts.
          ref={(el) => {
            if (el?.complete && el.naturalWidth > 0) markLoaded();
          }}
          loading={priority ? 'eager' : 'lazy'}
          fetchPriority={priority ? 'high' : undefined}
          decoding="async"
          draggable={false}
          onLoad={() => {
            markLoaded();
            onLoad?.();
          }}
          onError={() => {
            markErrored();
            onError?.();
          }}
          style={{
            ...layer,
            opacity: loaded ? 1 : 0,
            transition: `opacity ${duration}ms ease`,
          }}
        />
      ) : null}
    </View>
  );
}
