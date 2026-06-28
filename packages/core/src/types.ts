// Wire types shared by every LUMA client. These MUST stay in sync with the
// Rust server's JSON model (see server/src/model.rs).

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
export type MediaKind = 'movie' | 'episode' | 'video';
export type LibraryKind = 'movies' | 'shows' | 'mixed';

export interface VideoTrack {
  codec: VideoCodec;
  width: number | null;
  height: number | null;
  /** HDR10 / HLG signalled by the source. */
  hdr: boolean;
  /** 8 / 10 / 12. Null when unknown. */
  bitDepth: number | null;
}

export interface AudioTrack {
  /** Audio-relative index (0 = first audio track) — the selector key, and what
   * the server's per-track HLS remux maps with `-map 0:a:<index>`. */
  index: number;
  codec: AudioCodec;
  channels: number | null;
  language: string | null;
  /** Stream `title` tag ("Commentary", "Director's Cut", …), when present. */
  title?: string | null;
  /** Whether the container marks this as the default audio track. */
  default?: boolean;
}

export interface SubtitleTrack {
  language: string | null;
  codec: string;
}

/**
 * TMDB catalog metadata, resolved server-side during a scan. Image URLs are
 * locally-cached WebP paths (relative, e.g. `/api/images/<hash>.webp`) when the
 * server cached them, otherwise absolute `image.tmdb.org` URLs. Use
 * `LumaClient.posterFor` / `backdropFor` to resolve them against the origin.
 */
export interface Metadata {
  provider: 'tmdb' | string;
  tmdbId: number;
  imdbId?: string | null;
  title?: string | null;
  tagline?: string | null;
  overview?: string | null;
  releaseDate?: string | null;
  genres: string[];
  rating?: number | null;
  posterUrl?: string | null;
  backdropUrl?: string | null;
  /** Stylised title-treatment logo (transparent PNG path), when TMDB has one. */
  logoUrl?: string | null;
  /** Top-billed cast (TMDB credits). Absent on metadata resolved before the
   * server started fetching credits. */
  cast?: CastMember[];
  tmdbUrl: string;
}

/** A top-billed cast member surfaced in the detail page's "Distribution". */
export interface CastMember {
  name: string;
  /** The character they play, when TMDB provides one. */
  character?: string | null;
  /** Profile photo — a locally-cached WebP path (`/api/images/…`) when the
   * server cached it, else an absolute TMDB URL, else absent. Resolve against
   * the origin with `LumaClient.resolveArt`. */
  profileUrl?: string | null;
}

export interface MediaItem {
  id: string;
  title: string;
  kind: MediaKind;
  year: number | null;
  durationMs: number | null;
  /** Container/extension, e.g. "mkv", "mp4". */
  container: string;
  video: VideoTrack | null;
  /** Representative (first/default) audio track — kept for badges. */
  audio: AudioTrack | null;
  /** Every audio track, for the player's audio-track picker. */
  audioTracks: AudioTrack[];
  subtitles: SubtitleTrack[];
  /** Owning library id. */
  library: string;
  // --- show / episode grouping (null for movies) ---
  showId: string | null;
  showTitle: string | null;
  season: number | null;
  episode: number | null;
  /** Last episode number for multi-episode files (`S01E02-E03`). */
  episodeEnd: number | null;
  episodeTitle: string | null;
  /** Path relative to the library root. Null for built-in demo items. */
  relPath: string | null;
  /** ISO-8601. */
  addedAt: string;
  /** TMDB metadata (movies). Absent until background enrichment resolves it. */
  metadata?: Metadata | null;
}

/** A TV show aggregate (not a file) — built by grouping episodes during a scan. */
export interface Show {
  id: string;
  title: string;
  year: number | null;
  library: string;
  seasonCount: number;
  episodeCount: number;
  /** Representative video info (highest-res episode) for quality badges. */
  video: VideoTrack | null;
  addedAt: string;
  /** TMDB metadata (show-level). Absent until background enrichment resolves it. */
  metadata?: Metadata | null;
}

/** One season's episodes, sorted by episode number. */
export interface Season {
  number: number;
  episodes: MediaItem[];
}

/** `GET /api/shows/:id` payload. */
export interface ShowDetail {
  show: Show;
  seasons: Season[];
}

export interface Library {
  id: string;
  name: string;
  kind: LibraryKind;
  path: string;
  itemCount: number;
}

export interface Health {
  status: 'ok' | string;
  version: string;
  /** Whether the server found an `ffprobe` binary at startup. */
  ffprobe: boolean;
  libraries: number;
  items: number;
  shows: number;
}

export interface ScanResult {
  scanned: number;
  libraries: number;
  shows: number;
}

/** A user account (never carries the password). */
/** A granular capability. Mirrors the server's `Permission` enum; extend both
 * sides together (e.g. a future `stats.view`). Kept open (`| string`) so a
 * client built before a new permission still parses it. */
export type Permission =
  | 'users.manage'
  | 'library.manage'
  | 'settings.manage'
  | 'playback'
  | string;

export interface User {
  id: string;
  email: string;
  username: string;
  /** Cached WebP avatar (`/api/images/…`), or absent → fall back to initials. */
  avatarUrl?: string | null;
  /** Preferred UI locale (`"fr"` | `"en"`), synced across this account's
   * devices. Absent/null → clients fall back to the device/browser locale. */
  language?: string | null;
  /** Granted capabilities (no roles — capability-based). Clients unlock pages
   * and actions from this set. The owner account holds every permission. */
  permissions: Permission[];
  createdAt: string;
}

/** True if the user holds `perm`. Tolerates a missing `permissions` array so a
 * session persisted by an older client (before capabilities existed) degrades
 * to "no permissions" instead of crashing. */
export function hasPermission(user: Pick<User, 'permissions'>, perm: Permission): boolean {
  return user.permissions?.includes(perm) ?? false;
}

/** The public subset of a user for the "Qui regarde ?" picker (no email). */
export interface PublicUser {
  id: string;
  username: string;
  avatarUrl?: string | null;
}

/** A registration invitation (created by a user with `users.manage`). */
export interface Invite {
  token: string;
  permissions: Permission[];
  createdBy?: string | null;
  createdAt: string;
  /** Unix-seconds expiry. */
  expiresAt: number;
  used: boolean;
}

/** `POST /api/invites` result — the invite plus a ready-to-share join URL. */
export interface InviteCreated {
  token: string;
  /** `<web>/join?invite=…` when the server knows the web URL, else null. */
  url?: string | null;
  permissions: Permission[];
  expiresAt: number;
}

/** `{ token, user }` returned by register/login. */
export interface AuthResult {
  token: string;
  user: User;
}

/** One saved playback position. */
export interface ProgressEntry {
  itemId: string;
  positionMs: number;
  durationMs: number | null;
  updatedAt: string;
}

/** `POST /api/auth/quickconnect/initiate` — a device-pairing request. */
export interface QuickConnectInit {
  /** Short numeric code shown on the device. */
  code: string;
  /** Private handle the device polls with. */
  secret: string;
  expiresInSec: number;
  /** Web URL to approve the code (for a QR), when the server knows it. */
  authorizeUrl?: string | null;
}

/** `GET /api/auth/quickconnect/poll` result. */
export type QuickConnectStatus =
  | { status: 'pending' }
  | { status: 'expired' }
  | { status: 'authorized'; token: string; user: User };

/** A resumable item plus where to resume from (`GET /api/continue`). */
export interface ContinueItem {
  item: MediaItem;
  positionMs: number;
  durationMs: number | null;
  updatedAt: string;
}

/** `GET /api/status` — live scan/enrichment snapshot. */
export interface Activity {
  phase: 'idle' | 'scanning' | 'enriching' | 'ready' | string;
  scanning: boolean;
  libraries: number;
  shows: number;
  items: number;
  enrichDone: number;
  enrichTotal: number;
  lastScanAt: string | null;
}

// ===== Admin console =========================================================

/** A live playback session (`GET /api/admin/sessions`, dashboard "En cours de
 * lecture"). Clients heartbeat these via {@link LumaClient.pingPlayback}. */
export interface PlaybackSession {
  id: string;
  userId?: string;
  username: string;
  itemId: string;
  title: string;
  year: number | null;
  kind: string;
  showTitle?: string;
  season: number | null;
  episode: number | null;
  videoLabel: string;
  audioLabel: string;
  subtitle: string;
  /** Approx stream bitrate in Mb/s. */
  bitrate: number;
  /** `direct` | `transcode`. */
  mode: string;
  player: string;
  device: string;
  /** `LAN` | `WAN`. */
  network: string;
  ip: string;
  /** `playing` | `paused`. */
  state: string;
  positionMs: number;
  durationMs: number | null;
  /** Unix seconds. */
  startedAt: number;
}

/** What a client reports on each playback heartbeat. */
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

/** Server identity + uptime for the admin sidebar status card. */
export interface ServerInfo {
  name: string;
  hostname: string;
  version: string;
  uptimeSec: number;
  online: boolean;
  sessions: number;
}

/** Rolling CPU/RAM/bandwidth history (oldest → newest). Percentages 0..100. */
export interface MetricsSeries {
  cpuLuma: number[];
  cpuSystem: number[];
  ramLuma: number[];
  ramSystem: number[];
  /** Mb/s. */
  bwLocal: number[];
  bwRemote: number[];
}

/** `GET /api/admin/metrics`. */
export interface MetricsSnapshot {
  cpuLuma: number;
  cpuSystem: number;
  ramLumaBytes: number;
  ramUsedBytes: number;
  ramTotalBytes: number;
  bwLocalMbps: number;
  bwRemoteMbps: number;
  uptimeSecs: number;
  series: MetricsSeries;
}

/** One mounted volume (`GET /api/admin/storage`). */
export interface Volume {
  name: string;
  mount: string;
  fs: string;
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
}

/** `GET /api/admin/storage`. */
export interface StorageInfo {
  volumes: Volume[];
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
  mediaBytes: number;
  cache: { dir: string; bytes: number; limit: string };
}

/** A full account row for the admin "Membres & partage" table. */
export interface AdminUser {
  id: string;
  email: string;
  username: string;
  avatarUrl?: string | null;
  permissions: Permission[];
  /** Derived display label: "Propriétaire" | "Membre" | "Restreint". */
  role: string;
  createdAt: string;
  lastSeen?: string | null;
  online: boolean;
}

/** `GET /api/admin/users`. */
export interface AdminUsers {
  users: AdminUser[];
  libraryCount: number;
}

/** A named, multi-folder library (`GET /api/admin/libraries`). */
export interface AdminLibrary {
  id: string;
  name: string;
  /** `film` | `tv` | `music` | `photo`. */
  kind: string;
  folders: string[];
  itemCount: number;
  sizeBytes: number;
  lastScan: string | null;
  autoScan: boolean;
}

/** Per-user watch aggregate (dashboard "Top des utilisateurs"). */
export interface TopUser {
  username: string;
  plays: number;
  watchedMs: number;
  filmsMs: number;
  tvMs: number;
}

/** One weekly bucket of the play-history chart. */
export interface HistoryBucket {
  label: string;
  filmsMs: number;
  tvMs: number;
}

/** `GET /api/admin/stats/history`. */
export interface HistoryStats {
  buckets: HistoryBucket[];
  totalFilmsMs: number;
  totalTvMs: number;
}

/** `GET /api/admin/stats/overview`. */
export interface AdminOverview {
  users: number;
  online: number;
  invites: number;
  items: number;
  shows: number;
  libraries: number;
}

/** One editable (or read-only) settings row. */
export interface SettingRow {
  key: string;
  label: string;
  desc?: string;
  /** `toggle` | `select` | `text` | `value`. */
  kind: string;
  options?: string[];
  value: unknown;
  /** Whether the server actually enforces this setting (vs stored-only). */
  applied: boolean;
}

/** A titled group of settings rows. */
export interface SettingGroup {
  title: string;
  desc?: string;
  rows: SettingRow[];
}

/** `GET /api/admin/settings?view=…`. */
export interface SettingsView {
  view: string;
  groups: SettingGroup[];
}
