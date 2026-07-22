// KROMA brand lockup: "KR" + the chromatic wheel as the O + "MA", drawn from
// the official export's outlines so it renders identically offline. Path data
// mirrors packages/ui/src/components/kromaLockupPaths.ts (canvas 458x100).

import { View } from 'react-native';
import Svg, { Path } from 'react-native-svg';
import { colors } from '../lib/theme';
import { KromaWheel } from './KromaWheel';

const KR_PATH =
  'M19.32 90.2H0V11H19.32V45.68C23.24 43.92 26.88 41.72 30.24 39.08C33.6 36.44 36.58 33.56 39.18 30.44C41.78 27.32 43.94 24.1 45.66 20.78C47.38 17.46 48.64 14.2 49.44 11H71.76C70.8 14.68 69.24 18.42 67.08 22.22C64.92 26.02 62.32 29.66 59.28 33.14C56.24 36.62 52.98 39.72 49.5 42.44C46.02 45.16 42.48 47.32 38.88 48.92V50.72C42.56 50.72 45.82 51.12 48.66 51.92C51.5 52.72 54.02 53.96 56.22 55.64C58.42 57.32 60.34 59.42 61.98 61.94C63.62 64.46 65.04 67.48 66.24 71L73.08 90.2H51.12L46.68 74.48C45.72 70.96 44.44 68.18 42.84 66.14C41.24 64.1 39.12 62.62 36.48 61.7C33.84 60.78 30.44 60.32 26.28 60.32H19.32V90.2ZM101.28 90.2H81.96V11H115.68C120.08 11 124.08 11.34 127.68 12.02C131.28 12.7 134.46 13.7 137.22 15.02C139.98 16.34 142.32 17.94 144.24 19.82C146.16 21.7 147.6 23.86 148.56 26.3C149.52 28.74 150 31.44 150 34.4C150 37.2 149.56 39.72 148.68 41.96C147.8 44.2 146.46 46.16 144.66 47.84C142.86 49.52 140.6 50.88 137.88 51.92C135.16 52.96 132 53.72 128.4 54.2V56C132.96 56.48 136.5 57.5 139.02 59.06C141.54 60.62 143.48 62.68 144.84 65.24C146.2 67.8 147.4 70.88 148.44 74.48L153 90.2H131.52L128.04 75.68C127.32 72.48 126.34 70 125.1 68.24C123.86 66.48 122.28 65.26 120.36 64.58C118.44 63.9 116.04 63.56 113.16 63.56H101.28V90.2ZM101.28 25.64V49.28H114.36C119.24 49.28 123.04 48.32 125.76 46.4C128.48 44.48 129.84 41.48 129.84 37.4C129.84 33.4 128.6 30.44 126.12 28.52C123.64 26.6 119.88 25.64 114.84 25.64H101.28Z';

const MA_PATH =
  'M287 90.2H269V11H297.92L318.56 68H319.16L339.32 11H366.92V90.2H348.68L349.76 30.68H348.44L325.52 90.2H309.68L287.24 30.68H285.92L287 90.2ZM396.44 90.2H375.32L402.2 11H430.52L457.4 90.2H436.28L431.84 75.32H400.88L396.44 90.2ZM404.72 62.36H428L417.32 26.12H415.4L404.72 62.36Z';

const LOCKUP = {
  height: 100,
  krWidth: 153,
  maX: 269,
  maWidth: 188.4,
  gapLeft: 6,
  gapRight: 10,
} as const;

export function KromaLockup({ height = 40 }: Readonly<{ height?: number }>) {
  const s = height / LOCKUP.height;
  return (
    <View style={{ flexDirection: 'row', alignItems: 'center' }}>
      <Svg
        width={LOCKUP.krWidth * s}
        height={height}
        viewBox={`0 0 ${LOCKUP.krWidth} ${LOCKUP.height}`}
      >
        <Path d={KR_PATH} fill={colors.text} />
      </Svg>
      <View style={{ marginLeft: LOCKUP.gapLeft * s, marginRight: LOCKUP.gapRight * s }}>
        <KromaWheel size={height} />
      </View>
      <Svg
        width={LOCKUP.maWidth * s}
        height={height}
        viewBox={`${LOCKUP.maX} 0 ${LOCKUP.maWidth} ${LOCKUP.height}`}
      >
        <Path d={MA_PATH} fill={colors.text} />
      </Svg>
    </View>
  );
}
