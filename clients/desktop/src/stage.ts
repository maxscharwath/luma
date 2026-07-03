// The shared @luma/tv UI is authored on a fixed 1920x1080 canvas (mostly px-sized
// 10-foot layout). A real TV webview gets that canvas from `<meta viewport
// width=1920>` and the panel scales it in hardware. SteamOS runs *desktop* Chromium,
// which ignores that meta, so we reproduce the scaling ourselves: render #root at
// exactly 1920x1080 and `transform: scale()` it to fit the current screen,
// letterboxed. Works handheld (1280x800, scale ~0.667) and docked (1080p/4K).
//
// This is the same technique as the dev-only `tvFrame` plugin (clients/tv-frame.vite),
// but shipped in production because the Deck genuinely needs it. The `transform` on
// #root also makes it the containing block for the app's `position: fixed`
// full-screen layers (home / player / detail), so they fill the stage, not the
// raw viewport.
//
// Caveat (inherited from the TV frame): a few `vh`-based `clamp()`s in the TV CSS
// resolve against the real window rather than the stage, so they drift slightly on
// heavy up/down-scale. The fixed-px bulk of the layout stays pixel-faithful.

const STAGE_W = 1920;
const STAGE_H = 1080;

/** Install the self-scaling 1920x1080 stage. Call once before mounting. */
export function installStage(): void {
  // Transparent ONLY when a native mpv window renders behind the UI (the Linux
  // desktop shell): the player screen is transparent so the video shows through. On
  // macOS we play in an in-page <video> in an opaque window, and the plain browser
  // kiosk uses black letterbox bars - both want an opaque backdrop.
  const inTauri = '__TAURI_INTERNALS__' in globalThis || '__TAURI__' in globalThis;
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  const mpvBehind = inTauri && /Linux/i.test(ua) && !/Android/i.test(ua);
  const bg = mpvBehind ? 'transparent' : '#000';
  const style = document.createElement('style');
  style.textContent = `
    html, body { height: 100%; margin: 0; overflow: hidden; background: ${bg}; }
    #root {
      position: fixed; top: 50%; left: 50%;
      width: ${STAGE_W}px; height: ${STAGE_H}px;
      transform: translate(-50%, -50%) scale(var(--luma-stage-scale, 1));
      transform-origin: center center;
      overflow: hidden;
    }
  `;
  document.head.appendChild(style);

  const apply = () => {
    const scale = Math.min(window.innerWidth / STAGE_W, window.innerHeight / STAGE_H);
    document.documentElement.style.setProperty('--luma-stage-scale', String(scale));
  };
  apply();
  window.addEventListener('resize', apply);
}
