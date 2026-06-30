// Wire types shared by every LUMA client.
//
// Almost everything is GENERATED from the Rust server's `#[derive(TS)]` structs
// (the single source of truth) see `./generated` and `scripts/gen-types.sh`
// and re-exported below so consumers keep importing from `@luma/core`. What
// remains here is the handful of things codegen can't express: a request body the
// client *sends*, two open-union `codec` aliases, and a runtime helper.

export * from './generated';

import type { Permission, User } from './generated';

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
  state?: 'playing' | 'paused';
  mode?: 'direct' | 'transcode';
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
