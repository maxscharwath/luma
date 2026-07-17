// Wire types shared by every KROMA client.
//
// Every wire type is defined by a zod schema in `./schemas` (the single source of
// truth) and re-exported below so consumers keep importing from `@kroma/core`.
// What remains here is the handful of things the schemas don't express: two
// open-union `codec` aliases, a request body the client *sends*, and a runtime
// helper.

export * from './schemas';

import type { Permission, User } from './schemas';

/** Convenience open unions over the wire `codec` strings (kept open so a client
 * built before a new codec still parses it; the server sends a plain string). */
export type VideoCodec = 'hevc' | 'h264' | 'av1' | 'vp9' | 'mpeg2' | 'mpeg4' | string;
export type AudioCodec =
  | 'aac'
  | 'eac3'
  | 'ac3'
  | 'dts'
  | 'truehd'
  | 'flac'
  | 'opus'
  | 'mp3'
  | string;

/** What a client reports on each playback heartbeat (`POST /api/playback/ping`). */
export interface PlaybackPing {
  sessionId: string;
  itemId: string;
  positionMs: number;
  durationMs?: number | null;
  state?: 'playing' | 'paused' | 'buffering';
  /** `direct` (range copy) · `remux` (HLS, video+audio copied) · `transcode`
   * (HLS, audio re-encoded to AAC video is never transcoded). */
  mode?: 'direct' | 'remux' | 'transcode';
  player?: string;
  device?: string;
  audio?: string;
  subtitle?: string;
}

/** True if the user holds `perm`. Tolerates a missing `permissions` array so a
 * session persisted by an older client (before capabilities existed) degrades
 * to "no permissions" instead of crashing. */
export function hasPermission(user: Pick<User, 'permissions'>, perm: Permission): boolean {
  return user.permissions?.includes(perm) ?? false;
}
