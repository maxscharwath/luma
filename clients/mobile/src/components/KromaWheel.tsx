// The chromatic wheel "O" of the Kroma lockup, as react-native-svg. Sector
// geometry is the same annular-sector math as scripts/brand/gen-brand-assets.ts
// (hub/outer ratio 17.045/50).

import Svg, { Path } from 'react-native-svg';
import { WHEEL_COLORS } from '../lib/theme';

const round2 = (n: number) => Math.round(n * 100) / 100;

function wheelSectors(cx: number, cy: number, R: number, r: number): string[] {
  const rad = (deg: number) => (deg * Math.PI) / 180;
  const pt = (radius: number, deg: number) => [
    round2(cx + radius * Math.sin(rad(deg))),
    round2(cy - radius * Math.cos(rad(deg))),
  ];
  const out: string[] = [];
  for (let i = 0; i < 6; i++) {
    const [a1, a2] = [i * 60, i * 60 + 60];
    const [ox1, oy1] = pt(R, a1);
    const [ox2, oy2] = pt(R, a2);
    const [ix1, iy1] = pt(r, a1);
    const [ix2, iy2] = pt(r, a2);
    out.push(
      `M${ix1} ${iy1} L${ox1} ${oy1} A${R} ${R} 0 0 1 ${ox2} ${oy2} L${ix2} ${iy2} A${r} ${r} 0 0 0 ${ix1} ${iy1} Z`,
    );
  }
  return out;
}

const SECTORS = wheelSectors(50, 50, 50, 17.045);

export function KromaWheel({ size = 64 }: Readonly<{ size?: number }>) {
  return (
    <Svg width={size} height={size} viewBox="0 0 100 100">
      {SECTORS.map((d, i) => (
        <Path key={WHEEL_COLORS[i]} d={d} fill={WHEEL_COLORS[i]} />
      ))}
    </Svg>
  );
}
