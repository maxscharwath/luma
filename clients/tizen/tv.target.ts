import type { TvTarget } from '../tv-build/shell';

// Samsung Tizen. Modern tier only: Tizen 8+ (Chromium 108+, 2024 models) - the
// AVPlay engine and Smart Hub preview both assume recent firmware anyway.
// `deviceDev` honors KROMA_TV_DEVICE=1 for on-device live-dev over the LAN
// (scripts/dev-device.sh + `make dev-shell`).
export const target: TvTarget = {
  platform: 'tizen',
  port: 5174,
  deviceDev: true,
};
