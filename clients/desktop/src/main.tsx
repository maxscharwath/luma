import '@luma/tv/tv.css';
import { mountTv } from '@luma/tv';
import { startGamepadBridge } from './gamepad';
import { installStage } from './stage';

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
  // toggles `.luma-native-surface` (see tv.css) to go transparent only while a native
  // video plays.
  const base = document.createElement('style');
  base.textContent = 'html, body, #root { background: var(--luma-bg, #0a0a0c); }';
  document.head.appendChild(base);
}

// The Deck is driven by a gamepad, not a remote. Bridge the Gamepad API onto the
// same synthetic key events the shared TV nav already listens for.
startGamepadBridge();

mountTv({ platform: 'Desktop' });
