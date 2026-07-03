import '@luma/tv/tv.css';
import { mountTv } from '@luma/tv';
import { startGamepadBridge } from './gamepad';
import { installStage } from './stage';

// SteamOS' desktop Chromium ignores a fixed viewport, so scale the authored
// 1920x1080 TV canvas to the Deck's screen ourselves.
installStage();

// The Deck is driven by a gamepad, not a remote. Bridge the Gamepad API onto the
// same synthetic key events the shared TV nav already listens for.
startGamepadBridge();

mountTv({ platform: 'Desktop' });
