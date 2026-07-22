// <Img> on the native targets (Apple TV, Android TV).
//
// React Native's <Image> plus an Animated opacity fade, driven by the SAME
// cross-fade state machine the web renderer uses, so a hero swap has identical
// timing on every platform. `object-position` is reproduced by measuring the box
// and the artwork and computing the cover rectangle (see focal.ts), because
// resizeMode="cover" is hard-centred.

import { useMemo, useRef, useState } from 'react';
import {
  Animated,
  type LayoutChangeEvent,
  type NativeSyntheticEvent,
  Image as RNImage,
  View,
} from 'react-native';
import { absoluteFill } from '../tokens';
import { useCrossFade } from './crossfade';
import { gradient } from './css';
import { coverRect, parsePosition } from './focal';
import { IMG_FADE_MS, type ImgProps } from './img-types';

interface Size {
  width: number;
  height: number;
}

/** RN reports the decoded artwork's intrinsic size on the load event. */
type LoadEvent = NativeSyntheticEvent<{ source: Size }>;

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
  onLoad,
  onError,
}: Readonly<ImgProps>) {
  const { loaded, errored, under, markLoaded, markErrored } = useCrossFade(src, duration);
  const [box, setBox] = useState<Size | null>(null);
  const [natural, setNatural] = useState<Size | null>(null);
  const opacity = useRef(new Animated.Value(0)).current;

  const focal = useMemo(() => parsePosition(position), [position]);
  // `contain` never overflows, so it needs no focal maths at all.
  const rect = fit === 'cover' ? coverRect(box, natural, focal) : null;

  const onBoxLayout = (e: LayoutChangeEvent) => {
    const { width, height } = e.nativeEvent.layout;
    setBox((prev) => (prev?.width === width && prev.height === height ? prev : { width, height }));
  };

  const handleLoad = (e: LoadEvent) => {
    setNatural(e.nativeEvent.source);
    markLoaded();
    Animated.timing(opacity, {
      toValue: 1,
      duration,
      useNativeDriver: true,
    }).start();
    onLoad?.();
  };

  // With a known cover rectangle the geometry is already exact, so the image is
  // stretched into it; before that we fall back to a plain centred cover.
  const layer = rect ? { position: 'absolute' as const, ...rect } : absoluteFill;
  const mode = rect ? ('stretch' as const) : fit;

  return (
    <View
      onLayout={onBoxLayout}
      style={[
        fill ? absoluteFill : null,
        { overflow: 'hidden' },
        radius === undefined ? null : { borderRadius: radius },
        background === undefined ? null : gradient(background),
        style,
      ]}
    >
      {under && under !== src ? (
        <RNImage key="under" source={{ uri: under }} resizeMode={mode} style={layer} />
      ) : null}

      {src && !errored ? (
        <Animated.Image
          key={src}
          source={{ uri: src }}
          accessibilityLabel={alt || undefined}
          accessible={alt.length > 0}
          resizeMode={mode}
          onLoad={handleLoad}
          onError={() => {
            markErrored();
            onError?.();
          }}
          style={[layer, { opacity: loaded ? opacity : 0 }]}
        />
      ) : null}
    </View>
  );
}
