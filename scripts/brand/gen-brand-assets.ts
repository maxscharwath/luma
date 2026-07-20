// Regenerates every Kroma brand asset from the official lockup export
// (.github/assets/kroma-lockup-source.svg): outlined KR / MA letters + the
// chromatic wheel O (rebuilt mask-free as annular sectors, hub/outer ratio
// identical to the export: 17.045/50 = 15/44).
//
// Run:  bunx --yes sharp@latest >/dev/null 2>&1 || bun add -D sharp
//       bun scripts/brand/gen-brand-assets.ts
// Then regenerate the desktop set:
//       cd clients/desktop && bunx tauri icon <emitted tauri-source-1024.png>
//
// The in-app lockup is NOT generated from here: @kroma/ui <Logo>/<KromaMark>
// render it live (webfont + inline SVG) with the same metrics.
import { fileURLToPath } from 'node:url';
import sharp from 'sharp';

const REPO = fileURLToPath(new URL('../..', import.meta.url)).replace(/\/$/, '');
const DIR = import.meta.dir;
const INK = '#0A0A0C';
const IVORY = '#F4F3F0';

export const WHEEL_COLORS = ['#F2685C', '#F4B642', '#5FBF8F', '#4F9DE0', '#6366F1', '#A855F7'];

// Official letter outlines, verbatim from Frame 2.svg (canvas 458x100, caps y 11..90.2).
const KR_D =
  'M19.32 90.2H0V11H19.32V45.68C23.24 43.92 26.88 41.72 30.24 39.08C33.6 36.44 36.58 33.56 39.18 30.44C41.78 27.32 43.94 24.1 45.66 20.78C47.38 17.46 48.64 14.2 49.44 11H71.76C70.8 14.68 69.24 18.42 67.08 22.22C64.92 26.02 62.32 29.66 59.28 33.14C56.24 36.62 52.98 39.72 49.5 42.44C46.02 45.16 42.48 47.32 38.88 48.92V50.72C42.56 50.72 45.82 51.12 48.66 51.92C51.5 52.72 54.02 53.96 56.22 55.64C58.42 57.32 60.34 59.42 61.98 61.94C63.62 64.46 65.04 67.48 66.24 71L73.08 90.2H51.12L46.68 74.48C45.72 70.96 44.44 68.18 42.84 66.14C41.24 64.1 39.12 62.62 36.48 61.7C33.84 60.78 30.44 60.32 26.28 60.32H19.32V90.2ZM101.28 90.2H81.96V11H115.68C120.08 11 124.08 11.34 127.68 12.02C131.28 12.7 134.46 13.7 137.22 15.02C139.98 16.34 142.32 17.94 144.24 19.82C146.16 21.7 147.6 23.86 148.56 26.3C149.52 28.74 150 31.44 150 34.4C150 37.2 149.56 39.72 148.68 41.96C147.8 44.2 146.46 46.16 144.66 47.84C142.86 49.52 140.6 50.88 137.88 51.92C135.16 52.96 132 53.72 128.4 54.2V56C132.96 56.48 136.5 57.5 139.02 59.06C141.54 60.62 143.48 62.68 144.84 65.24C146.2 67.8 147.4 70.88 148.44 74.48L153 90.2H131.52L128.04 75.68C127.32 72.48 126.34 70 125.1 68.24C123.86 66.48 122.28 65.26 120.36 64.58C118.44 63.9 116.04 63.56 113.16 63.56H101.28V90.2ZM101.28 25.64V49.28H114.36C119.24 49.28 123.04 48.32 125.76 46.4C128.48 44.48 129.84 41.48 129.84 37.4C129.84 33.4 128.6 30.44 126.12 28.52C123.64 26.6 119.88 25.64 114.84 25.64H101.28Z';
const MA_D =
  'M287 90.2H269V11H297.92L318.56 68H319.16L339.32 11H366.92V90.2H348.68L349.76 30.68H348.44L325.52 90.2H309.68L287.24 30.68H285.92L287 90.2ZM396.44 90.2H375.32L402.2 11H430.52L457.4 90.2H436.28L431.84 75.32H400.88L396.44 90.2ZM404.72 62.36H428L417.32 26.12H415.4L404.72 62.36Z';

const LOCKUP_W = 457.4;
const LOCKUP_H = 100;

const round2 = (n: number) => Math.round(n * 100) / 100;

// Annular wheel sectors around (cx, cy): mask-free equivalent of the export's
// masked wheel. Default = the Frame 2 frame (cx 209, cy 50, R 50, r 17.045).
export function wheelSectors(cx = 209, cy = 50, R = 50, r = 17.045): string[] {
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

function wheelSvgPaths(cx?: number, cy?: number, R?: number, r?: number): string {
  return wheelSectors(cx, cy, R, r)
    .map((d, i) => `  <path d="${d}" fill="${WHEEL_COLORS[i]}"/>`)
    .join('\n');
}

// Full lockup fragment (letters + wheel) in Frame 2 coordinates.
function lockupPaths(textFill: string): string {
  return `  <path d="${KR_D}" fill="${textFill}"/>
${wheelSvgPaths()}
  <path d="${MA_D}" fill="${textFill}"/>`;
}

// ---- 1. canonical lockup SVGs (.github/assets) -------------------------------

function lockupSvg(textFill: string): string {
  return `<svg width="458" height="100" viewBox="0 0 458 100" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="KROMA">
${lockupPaths(textFill)}
</svg>
`;
}
await Bun.write(`${REPO}/.github/assets/kroma-lockup-ink.svg`, lockupSvg(INK));
await Bun.write(`${REPO}/.github/assets/kroma-lockup-ivory.svg`, lockupSvg(IVORY));
console.log('wrote canonical lockups (ink + ivory)');

// ---- 2. .github/assets/logo.svg ----------------------------------------------

{
  const w = 232;
  const s = round2((w / LOCKUP_W) * 10000) / 10000;
  const x = round2((280 - w) / 2);
  const y = round2((84 - LOCKUP_H * s) / 2);
  const svg = `<svg width="280" height="84" viewBox="0 0 280 84" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="KROMA">
  <rect x="0.75" y="0.75" width="278.5" height="82.5" rx="18" fill="${INK}" stroke="#F4B642" stroke-opacity="0.18" stroke-width="1.5"/>
  <!-- official KROMA lockup (Frame 2 export): outlined letters + chromatic-wheel O -->
  <g transform="translate(${x},${y}) scale(${s})">
${lockupPaths(IVORY)}
  </g>
</svg>
`;
  await Bun.write(`${REPO}/.github/assets/logo.svg`, svg);
  console.log('wrote logo.svg');
}

// ---- 3. .github/assets/banner.svg --------------------------------------------

{
  const w = 660;
  const s = round2((w / LOCKUP_W) * 10000) / 10000;
  const x = round2((1280 - w) / 2);
  const y = 118;
  const svg = `<svg width="1280" height="420" viewBox="0 0 1280 420" fill="none" xmlns="http://www.w3.org/2000/svg" role="img" aria-label="KROMA self-hosted, direct-play, HEVC-first media streaming">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="420" gradientUnits="userSpaceOnUse">
      <stop offset="0" stop-color="#101014"/>
      <stop offset="1" stop-color="#08080A"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.4" r="0.55">
      <stop offset="0" stop-color="#F4B642" stop-opacity="0.2"/>
      <stop offset="0.55" stop-color="#F4B642" stop-opacity="0.05"/>
      <stop offset="1" stop-color="#F4B642" stop-opacity="0"/>
    </radialGradient>
  </defs>

  <rect x="0" y="0" width="1280" height="420" rx="28" fill="url(#bg)"/>
  <rect x="0" y="0" width="1280" height="420" rx="28" fill="url(#glow)"/>
  <rect x="0.75" y="0.75" width="1278.5" height="418.5" rx="27.25" fill="none" stroke="#F4B642" stroke-opacity="0.16" stroke-width="1.5"/>

  <!-- official KROMA lockup (Frame 2 export) -->
  <g transform="translate(${x},${y}) scale(${s})">
${lockupPaths(IVORY)}
  </g>

  <!-- tagline -->
  <text x="640" y="312" text-anchor="middle"
        font-family="'Segoe UI', Helvetica, Arial, sans-serif" font-size="25" font-weight="500"
        letter-spacing="0.4" fill="#A6A5A2">Self-hosted · direct-play · HEVC-first media streaming</text>

  <!-- platform overline -->
  <text x="640" y="364" text-anchor="middle"
        font-family="'Segoe UI', Helvetica, Arial, sans-serif" font-size="15" font-weight="700"
        letter-spacing="6" fill="#F4B642">WEB · SAMSUNG TIZEN · LG webOS · SYNOLOGY NAS</text>
</svg>
`;
  await Bun.write(`${REPO}/.github/assets/banner.svg`, svg);
  console.log('wrote banner.svg');
}

// ---- 4. Android TV banner vector drawable -------------------------------------

{
  const w = 240;
  const s = Math.round((w / LOCKUP_W) * 100000) / 100000;
  const tx = round2((320 - w) / 2);
  const ty = round2((180 - LOCKUP_H * s) / 2);
  const xml = `<?xml version="1.0" encoding="utf-8"?>
<!-- KROMA launcher banner (320x180): the official horizontal lockup (outlined
     letters + chromatic-wheel O, mask-free annular sectors). -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="320dp"
    android:height="180dp"
    android:viewportWidth="320"
    android:viewportHeight="180">

    <path
        android:pathData="M0,0h320v180h-320z"
        android:fillColor="#0A0A0C" />

    <group
        android:translateX="${tx}"
        android:translateY="${ty}"
        android:scaleX="${s}"
        android:scaleY="${s}">
        <path android:pathData="${KR_D}" android:fillColor="#F4F3F0" />
${wheelSectors()
  .map((d, i) => `        <path android:pathData="${d}" android:fillColor="${WHEEL_COLORS[i]}" />`)
  .join('\n')}
        <path android:pathData="${MA_D}" android:fillColor="#F4F3F0" />
    </group>
</vector>
`;
  await Bun.write(`${REPO}/clients/androidtv/android/app/src/main/res/drawable/tv_banner.xml`, xml);
  console.log('wrote tv_banner.xml');
}

// ---- 5. raster icons (wheel full-bleed in its own box) ------------------------

// Icon SVG: wheel centred, `symbolFrac` = true wheel diameter / icon size.
function iconSvg(size: number, bg: string, radiusFrac: number, symbolFrac: number): string {
  const shapes: string[] = [];
  if (bg !== 'none') {
    shapes.push(
      `<rect width="${size}" height="${size}" rx="${Math.round(size * radiusFrac)}" fill="${bg}"/>`,
    );
  }
  const d = size * symbolFrac;
  const c = size / 2;
  shapes.push(
    wheelSectors(c, c, d / 2, (d / 2) * (17.045 / 50))
      .map((p, i) => `<path d="${p}" fill="${WHEEL_COLORS[i]}"/>`)
      .join(''),
  );
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">${shapes.join('')}</svg>`;
}

async function png(
  outPath: string,
  size: number,
  bg: string,
  opts?: { radiusFrac?: number; symbolFrac?: number; alpha?: boolean },
) {
  const svg = iconSvg(size, bg, opts?.radiusFrac ?? 0, opts?.symbolFrac ?? 0.6);
  let img = sharp(Buffer.from(svg), { density: 72 * Math.max(1, 1024 / size) }).resize(size, size);
  if (opts?.alpha === false) img = img.removeAlpha();
  await img.png().toFile(outPath);
  console.log('wrote', outPath);
}

await png(`${REPO}/clients/web/public/favicon-32.png`, 32, 'none', { symbolFrac: 1.0 });
await png(`${REPO}/clients/web/public/apple-touch-icon.png`, 180, INK);
await png(`${REPO}/clients/tizen/public/icon.png`, 512, INK);
await png(`${REPO}/clients/tizen/.dev-shell/icon.png`, 512, INK);
await png(`${REPO}/clients/webos/public/icon.png`, 80, INK, { symbolFrac: 0.64, alpha: false });
await png(`${REPO}/clients/webos/public/icon-large.png`, 130, INK, {
  symbolFrac: 0.64,
  alpha: false,
});
await png(`${REPO}/clients/synology/spk/PACKAGE_ICON.PNG`, 72, INK, {
  symbolFrac: 0.64,
  alpha: false,
});
await png(`${REPO}/clients/synology/spk/PACKAGE_ICON_256.PNG`, 256, INK, { alpha: false });
await png(`${DIR}/tauri-source-1024.png`, 1024, INK, { radiusFrac: 0.225 });

console.log('all assets emitted (official geometry)');
