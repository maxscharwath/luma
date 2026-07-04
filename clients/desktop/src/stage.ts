// The shared @luma/tv UI is authored on a fixed 1920x1080 canvas (px-sized 10-foot
// layout). On a FIXED-SCREEN shell - the Steam Deck or a fullscreen Chromium kiosk -
// we render #root at 1920x1080 and scale it to fit the screen, letterboxed with the
// app background. This is the "TV" behaviour. Desktop windows (macOS / Windows) skip
// it entirely (see main.tsx) and run free-size, like a web page.
//
// The `transform` on #root also makes it the containing block for the app's
// `position: fixed` full-screen layers (home / player / detail), so they fill the
// stage. Caveat: a few `vh`-based `clamp()`s in the TV CSS resolve against the real
// window, so they drift slightly on heavy scale.

const STAGE_W = 1920;
const STAGE_H = 1080;

/** Install the self-scaling 1920x1080 stage. Call only for fixed-screen shells. */
export function installStage(): void {
  // Transparent when a native mpv window renders behind the UI (the Linux desktop
  // shell); otherwise the app background fills the letterbox surround.
  const inTauri = '__TAURI_INTERNALS__' in globalThis || '__TAURI__' in globalThis;
  const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
  const mpvBehind = inTauri && /Linux/i.test(ua) && !/Android/i.test(ua);
  const bg = mpvBehind ? 'transparent' : 'var(--luma-bg, #0a0a0c)';

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
