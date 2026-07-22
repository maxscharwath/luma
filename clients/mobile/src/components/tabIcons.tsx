// Bottom-tab icons from the shared Tabler set (same family as the web client).

import IconDeviceTv from '@tabler/icons-react-native/dist/esm/icons/IconDeviceTv.mjs';
import IconHome from '@tabler/icons-react-native/dist/esm/icons/IconHome.mjs';
import IconMovie from '@tabler/icons-react-native/dist/esm/icons/IconMovie.mjs';
import IconSearch from '@tabler/icons-react-native/dist/esm/icons/IconSearch.mjs';
import IconUserCircle from '@tabler/icons-react-native/dist/esm/icons/IconUserCircle.mjs';
import type { ColorValue } from 'react-native';

interface TabIconProps {
  color: ColorValue;
  size?: number;
}

export function HomeTabIcon({ color, size = 24 }: Readonly<TabIconProps>) {
  return <IconHome width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function SearchTabIcon({ color, size = 24 }: Readonly<TabIconProps>) {
  return <IconSearch width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function FilmTabIcon({ color, size = 24 }: Readonly<TabIconProps>) {
  return <IconMovie width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function SeriesTabIcon({ color, size = 24 }: Readonly<TabIconProps>) {
  return <IconDeviceTv width={size} height={size} color={color} strokeWidth={1.8} />;
}

export function ProfileTabIcon({ color, size = 24 }: Readonly<TabIconProps>) {
  return <IconUserCircle width={size} height={size} color={color} strokeWidth={1.8} />;
}
