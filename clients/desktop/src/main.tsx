import '@kroma/tv/tv.css';
import { mountTv } from '@kroma/tv';
// Display-matched grade of the brand-intro film, bundled by THIS shell only: the
// Tauri window is transparent (native mpv plane behind the webview), which costs
// <video> its compositor fast path, so the shared 4K60 HEVC film decodes and
// downscales the slow way and stutters. 1080p60 is a quarter of the work and
// indistinguishable in a desktop window, and H.264 (not HEVC like the shared
// film) so Linux WebKitGTK can decode it at all (gstreamer1.0-libav; HEVC there
// hit the CSS fallback) and a software decode stays cheap. Same master,
// `scale=1920:1080` x264 crf18 re-encode (avc1 + faststart), see packages/ui
// KromaIntro/constants.ts.
import introFilm from './assets/kroma-intro-h264-1080.mp4';
import { startGamepadBridge } from './gamepad';
import { installStage } from './stage';
import { startUpdater } from './updater';

// Fixed 1920x1080 stage ONLY on a fixed-screen shell (the Steam Deck / a fullscreen
// Chromium kiosk - both Linux): there the 10-foot canvas is scaled to fill the panel.
// macOS / Windows desktop windows skip it and run FREE-SIZE, like a web page (the
// window resizes naturally, no forced ratio).
const ua = typeof navigator !== 'undefined' ? navigator.userAgent : '';
const fixedScreen = /Linux/i.test(ua) && !/Android/i.test(ua);
if (fixedScreen) {
  installStage();
} else {
  // Free-size desktop (macOS/Windows). On macOS the window is TRANSPARENT so the native
  // mpv video plane can show behind the player, so paint an opaque app background by
  // default - otherwise the transparent window shows through everywhere. The player
  // toggles `.kroma-native-surface` (see tv.css) to go transparent only while a native
  // video plays.
  const base = document.createElement('style');
  base.textContent = 'html, body, #root { background: var(--kroma-bg, #0a0a0c); }';
  document.head.appendChild(base);
}

// The Deck is driven by a gamepad, not a remote. Bridge the Gamepad API onto the
// same synthetic key events the shared TV nav already listens for.
startGamepadBridge();

mountTv({ platform: 'Desktop', introVideoSrc: introFilm });

// The frontend is alive: disarm the GPU-rendering crash guard for this boot
// (src-tauri/src/webview_gpu.rs). The command exists on the Linux shell only.
if (fixedScreen) {
  (
    globalThis as { __TAURI__?: { core?: { invoke?: (cmd: string) => Promise<unknown> } } }
  ).__TAURI__?.core
    ?.invoke?.('webview_boot_ok')
    .catch(() => undefined);
}

// Keep the app current from GitHub Releases (no-op in a browser dev run).
startUpdater();
