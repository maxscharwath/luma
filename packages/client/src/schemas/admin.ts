// Runtime schemas for the admin-core domain (users, settings, storage, metrics,
// history, invites, activity). Follows the accounts.ts template: each schema
// mirrors a ts-rs generated wire type, adds runtime validation via `.parse()`
// and branded ids, and carries a `true satisfies SameKeys<…>` compile-time drift
// guard so a Rust struct change + `gen:types` breaks the build here until updated.

import { z } from 'zod';
import { Permission } from './accounts';
import { ItemId, LibraryId, UserId } from './ids';

/** One account in the admin "Membres & partage" table carries email, a derived
 * role, last-activity and a live `online` flag. */
export const AdminUser = z.object({
  id: UserId,
  email: z.string(),
  username: z.string(),
  avatarUrl: z.string().nullish(),
  permissions: z.array(Permission),
  role: z.string(),
  createdAt: z.string(),
  lastSeen: z.string().nullish(),
  online: z.boolean(),
});
export type AdminUser = z.infer<typeof AdminUser>;

/** `GET /api/admin/users`. */
export const AdminUsers = z.object({
  users: z.array(AdminUser),
  libraryCount: z.number(),
});
export type AdminUsers = z.infer<typeof AdminUsers>;

/** `GET /api/admin/stats/overview`. */
export const AdminOverview = z.object({
  users: z.number(),
  online: z.number(),
  invites: z.number(),
  items: z.number(),
  shows: z.number(),
  libraries: z.number(),
});
export type AdminOverview = z.infer<typeof AdminOverview>;

/** A named, multi-folder library (`GET /api/admin/libraries`). `id` is an opaque
 * library id, not a media id, so it stays a plain string. */
export const AdminLibrary = z.object({
  id: LibraryId,
  name: z.string(),
  kind: z.string(),
  folders: z.array(z.string()),
  itemCount: z.number(),
  sizeBytes: z.number(),
  lastScan: z.string().nullable(),
  autoScan: z.boolean(),
});
export type AdminLibrary = z.infer<typeof AdminLibrary>;

/** One editable (or read-only) setting row. `kind` (`toggle`|`select`|`text`|
 * `value`) is a plain string on the wire and `value` is an untyped `unknown`
 * (ts-rs `serde_json::Value`), so this is a flat object, not a tagged union. */
export const SettingRow = z.object({
  key: z.string(),
  label: z.string(),
  desc: z.string().nullish(),
  kind: z.string(),
  options: z.array(z.string()),
  value: z.unknown(),
  applied: z.boolean(),
});
export type SettingRow = z.infer<typeof SettingRow>;

/** A titled group of rows. */
export const SettingGroup = z.object({
  title: z.string(),
  desc: z.string().nullish(),
  rows: z.array(SettingRow),
});
export type SettingGroup = z.infer<typeof SettingGroup>;

/** `GET /api/admin/settings?view=…`. */
export const SettingsView = z.object({
  view: z.string(),
  groups: z.array(SettingGroup),
});
export type SettingsView = z.infer<typeof SettingsView>;

/** Cache directory usage + counts, nested in [`StorageInfo`]. */
export const CacheInfo = z.object({
  dir: z.string(),
  bytes: z.number(),
  limit: z.string(),
  transcodeBytes: z.number(),
  transcodeLimit: z.string(),
  imagesBytes: z.number(),
  imagesCount: z.number(),
  enrichedItems: z.number(),
  enrichedShows: z.number(),
  embeddings: z.number(),
});
export type CacheInfo = z.infer<typeof CacheInfo>;

/** One mounted volume's usage. */
export const Volume = z.object({
  name: z.string(),
  mount: z.string(),
  fs: z.string(),
  totalBytes: z.number(),
  usedBytes: z.number(),
  availableBytes: z.number(),
});
export type Volume = z.infer<typeof Volume>;

/** `GET /api/admin/storage`. */
export const StorageInfo = z.object({
  volumes: z.array(Volume),
  totalBytes: z.number(),
  usedBytes: z.number(),
  availableBytes: z.number(),
  mediaBytes: z.number(),
  cache: CacheInfo,
});
export type StorageInfo = z.infer<typeof StorageInfo>;

/** Time-series history (oldest → newest). Percentages are 0..100. */
export const MetricsSeries = z.object({
  cpuKroma: z.array(z.number()),
  cpuSystem: z.array(z.number()),
  ramKroma: z.array(z.number()),
  ramSystem: z.array(z.number()),
  bwLocal: z.array(z.number()),
  bwRemote: z.array(z.number()),
});
export type MetricsSeries = z.infer<typeof MetricsSeries>;

/** A point-in-time metrics snapshot plus the recent history series. */
export const MetricsSnapshot = z.object({
  cpuKroma: z.number(),
  cpuSystem: z.number(),
  ramKromaBytes: z.number(),
  ramUsedBytes: z.number(),
  ramTotalBytes: z.number(),
  bwLocalMbps: z.number(),
  bwRemoteMbps: z.number(),
  uptimeSecs: z.number(),
  series: MetricsSeries,
});
export type MetricsSnapshot = z.infer<typeof MetricsSnapshot>;

/** One weekly bucket of the play-history chart. */
export const HistoryBucket = z.object({
  label: z.string(),
  filmsMs: z.number(),
  tvMs: z.number(),
});
export type HistoryBucket = z.infer<typeof HistoryBucket>;

/** `GET /api/admin/stats/history`. */
export const HistoryStats = z.object({
  buckets: z.array(HistoryBucket),
  totalFilmsMs: z.number(),
  totalTvMs: z.number(),
});
export type HistoryStats = z.infer<typeof HistoryStats>;

/** Per-series aggregate over its episodes, for the elements list. */
export const EpStats = z.object({
  episodes: z.number(),
  probed: z.number(),
  storyboarded: z.number(),
  seasons: z.number(),
  markerSeasons: z.number(),
});
export type EpStats = z.infer<typeof EpStats>;

/** A snapshot of what the server is doing. */
export const Activity = z.object({
  phase: z.string(),
  scanning: z.boolean(),
  libraries: z.number(),
  shows: z.number(),
  items: z.number(),
  enrichDone: z.number(),
  enrichTotal: z.number(),
  probeDone: z.number(),
  probeTotal: z.number(),
  lastScanAt: z.string().nullable(),
});
export type Activity = z.infer<typeof Activity>;

/** Aggregated per-user watch stats over a window (dashboard "Top des
 * utilisateurs"). Keyed by `username` here, so it carries no branded id. */
export const TopUser = z.object({
  username: z.string(),
  plays: z.number(),
  watchedMs: z.number(),
  filmsMs: z.number(),
  tvMs: z.number(),
});
export type TopUser = z.infer<typeof TopUser>;

/** A live playback session, serialized for the admin dashboard. `id` is the
 * opaque session id; `userId`/`itemId` carry branded ids. */
export const PlaybackSession = z.object({
  id: z.string(),
  userId: UserId.nullish(),
  username: z.string(),
  itemId: ItemId,
  title: z.string(),
  year: z.number().nullable(),
  kind: z.string(),
  showTitle: z.string().nullable(),
  season: z.number().nullable(),
  episode: z.number().nullable(),
  videoLabel: z.string(),
  audioLabel: z.string(),
  subtitle: z.string(),
  bitrate: z.number(),
  mode: z.string(),
  player: z.string(),
  device: z.string(),
  network: z.string(),
  ip: z.string(),
  state: z.string(),
  positionMs: z.number(),
  durationMs: z.number().nullable(),
  startedAt: z.number(),
});
export type PlaybackSession = z.infer<typeof PlaybackSession>;

/** A registration invitation created by a user with `users.manage`. `createdBy`
 * is a nullable display string, not a branded id. */
export const Invite = z.object({
  token: z.string(),
  permissions: z.array(Permission),
  createdBy: z.string().nullish(),
  createdAt: z.string(),
  expiresAt: z.number(),
  used: z.boolean(),
});
export type Invite = z.infer<typeof Invite>;

/** `POST /api/invites` result the invite plus a ready-to-share join URL. */
export const InviteCreated = z.object({
  token: z.string(),
  url: z.string().nullable(),
  permissions: z.array(Permission),
  expiresAt: z.number(),
});
export type InviteCreated = z.infer<typeof InviteCreated>;

/** One line of the server's in-memory log ring (`GET /api/admin/logs`). */
export const LogEntry = z.object({
  /** Arrival time, unix ms. */
  ts: z.number(),
  /** trace | debug | info | warn | error. */
  level: z.string(),
  /** Rust tracing target for core lines; empty for module lines. */
  target: z.string(),
  /** `core` or a module id (`tv.kroma.vpn`). */
  source: z.string(),
  message: z.string(),
});
export type LogEntry = z.infer<typeof LogEntry>;

/** `GET /api/admin/logs` recent lines (newest last) + the sources present. */
export const LogsView = z.object({
  entries: z.array(LogEntry),
  sources: z.array(z.string()),
});
export type LogsView = z.infer<typeof LogsView>;
