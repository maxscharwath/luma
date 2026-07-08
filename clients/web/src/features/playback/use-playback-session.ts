import { usePlaybackHeartbeat } from '@luma/ui';
import { apiBase } from '#web/shared/lib/api';
import type { MovieView } from '#web/shared/lib/api';
import { useAuth } from '#web/shared/lib/auth';

// Web adapter over the shared playback heartbeat (@luma/ui): supplies the signed-in
// client, browser-UA labels, the offset-aware position, and drives the prompt ping
// off the player's React `playing` state. See `usePlaybackHeartbeat` for the loop.

interface Params {
  item: MovieView;
  /** Absolute current position in seconds (offset-aware). */
  getPosition: () => number;
  playing: boolean;
  /** True while the element is stalled/rebuffering reported as `buffering`. */
  waiting: boolean;
  /** Label of the selected audio track (viewer's choice, shown on the dashboard). */
  audioLabel?: string;
  /** Label of the selected subtitle track, or the "off" label when none. */
  subtitleLabel?: string;
  /** `direct` (range copy) · `remux` (HLS, streams copied) · `transcode`
   * (HLS, audio re-encoded to AAC). Video is never transcoded. */
  mode: 'direct' | 'remux' | 'transcode';
  /** Called when an admin terminates this session. `message` may be empty. */
  onTerminated?: (message: string) => void;
}

function uaInfo(): { player: string; device: string } {
  if (typeof navigator === 'undefined') return { player: 'LUMA Web', device: 'Navigateur' };
  const ua = navigator.userAgent;
  let player = 'Navigateur';
  if (/Edg\//.test(ua)) player = 'Edge';
  else if (/Firefox\//.test(ua)) player = 'Firefox';
  else if (/Chrome\//.test(ua)) player = 'Chrome';
  else if (/Safari\//.test(ua)) player = 'Safari';
  let device = 'Web';
  if (/iPhone|iPad/.test(ua)) device = 'iOS';
  else if (/Android/.test(ua)) device = 'Android';
  else if (/Mac OS X/.test(ua)) device = 'macOS';
  else if (/Windows/.test(ua)) device = 'Windows';
  else if (/Linux/.test(ua)) device = 'Linux';
  return { player, device };
}

export function usePlaybackSession(params: Params): void {
  const { client, user } = useAuth();
  const ua = uaInfo();
  usePlaybackHeartbeat({
    client,
    enabled: !!user,
    itemId: params.item.id,
    durationMs: params.item.durationMs ?? null,
    getPosition: params.getPosition,
    getState: () => {
      if (!params.playing) return 'paused';
      return params.waiting ? 'buffering' : 'playing';
    },
    getAudio: () => params.audioLabel,
    getSubtitle: () => params.subtitleLabel,
    mode: params.mode,
    player: ua.player,
    device: ua.device,
    eventsBaseUrl: apiBase(),
    idPrefix: 'web',
    // Ping promptly on any of these (not just the 10s beat) so buffering + a
    // track switch surface on the dashboard near-instantly.
    pingSignal: `${params.playing}|${params.waiting}|${params.audioLabel ?? ''}|${params.subtitleLabel ?? ''}`,
    onTerminated: (message) => params.onTerminated?.(message),
  });
}
