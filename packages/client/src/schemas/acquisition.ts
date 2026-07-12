// Runtime schemas for the acquisition domain (downloads, download clients,
// indexers, manual/interactive search, torrent analysis).
//
// Each schema mirrors a ts-rs generated wire type but adds runtime validation
// (via `.parse()`) and branded ids. The `true satisfies SameKeys<…>` lines are
// compile-time drift guards: change a Rust struct + rerun `gen:types` and the
// build breaks here until the schema is updated. See `./accounts` for the
// pattern and `./drift` for how the guards work.

import { z } from 'zod';
import { DownloadClientId, IndexerId, ItemId, RequestId } from './ids';
import { VpnStatusView } from './pipeline';

// ── Download clients ────────────────────────────────────────────────────────

/** One configured download client (password write-only). `kind` is an open
 * string on the wire (`rqbit` | `transmission` | `qbittorrent`). */
export const DownloadClientView = z.object({
  id: DownloadClientId,
  kind: z.string(),
  name: z.string(),
  url: z.string(),
  username: z.string(),
  hasPassword: z.boolean(),
  enabled: z.boolean(),
  priority: z.number(),
  createdAt: z.number(),
  builtin: z.boolean(),
});
export type DownloadClientView = z.infer<typeof DownloadClientView>;

/** `GET /api/admin/download-clients`. */
export const DownloadClientsView = z.object({
  clients: z.array(DownloadClientView),
  rqbitCompiled: z.boolean(),
});
export type DownloadClientsView = z.infer<typeof DownloadClientsView>;

/** `POST /api/admin/download-clients/:id/test` result. */
export const ClientTestResult = z.object({
  ok: z.boolean(),
  version: z.string().nullable(),
  error: z.string().nullable(),
});
export type ClientTestResult = z.infer<typeof ClientTestResult>;

/** Create/update body for a download client (all fields optional patch). */
export const SaveDownloadClientBody = z.object({
  kind: z.string().nullable(),
  name: z.string().nullable(),
  url: z.string().nullable(),
  username: z.string().nullable(),
  password: z.string().nullable(),
  enabled: z.boolean().nullable(),
  priority: z.number().nullable(),
});
export type SaveDownloadClientBody = z.infer<typeof SaveDownloadClientBody>;

// ── Torrent analysis ────────────────────────────────────────────────────────

/** One file inside an analyzed torrent, with its detected season/episode. */
export const TorrentFileView = z.object({
  index: z.number(),
  path: z.string(),
  sizeBytes: z.number(),
  isVideo: z.boolean(),
  season: z.number().nullable(),
  episode: z.number().nullable(),
});
export type TorrentFileView = z.infer<typeof TorrentFileView>;

/** `POST /api/admin/acquisition/analyze` result. `kind` is an open string
 * (`movie` | `episode` | `season` | `series` | `unknown`). */
export const TorrentAnalysis = z.object({
  kind: z.string(),
  seasons: z.array(z.number()),
  files: z.array(TorrentFileView),
});
export type TorrentAnalysis = z.infer<typeof TorrentAnalysis>;

// ── Downloads (queue) ───────────────────────────────────────────────────────

/** One download (grab) in the admin queue. `id` is a download-row id (no brand);
 * `infoHash` is an opaque torrent hash. `localId` is the catalog item once
 * imported. `kind`/`status` are open strings on the wire. */
export const DownloadView = z.object({
  id: z.string(),
  clientId: DownloadClientId,
  clientName: z.string(),
  requestId: RequestId.nullable(),
  kind: z.string(),
  title: z.string(),
  releaseTitle: z.string(),
  season: z.number().nullable(),
  episodes: z.array(z.number()).nullable(),
  status: z.string(),
  progress: z.number(),
  sizeBytes: z.number().nullable(),
  score: z.number().nullable(),
  error: z.string().nullable(),
  grabbedAt: z.number(),
  completedAt: z.number().nullable(),
  importedAt: z.number().nullable(),
  indexerName: z.string().nullable(),
  detailsUrl: z.string().nullable(),
  infoHash: z.string().nullable(),
  posterUrl: z.string().nullable(),
  localId: ItemId.nullable(),
});
export type DownloadView = z.infer<typeof DownloadView>;

/** `GET /api/admin/downloads`. */
export const DownloadsView = z.object({
  downloads: z.array(DownloadView),
  vpn: VpnStatusView.nullable(),
});
export type DownloadsView = z.infer<typeof DownloadsView>;

// ── Grab / manual add ───────────────────────────────────────────────────────

/** `POST /api/requests/:id/grab` body. `guid` is an opaque release id. */
export const GrabBody = z.object({
  guid: z.string(),
  indexerId: IndexerId,
});
export type GrabBody = z.infer<typeof GrabBody>;

/** `POST /api/admin/acquisition/add` body. `tmdbId` is a foreign numeric id. */
export const ManualAddBody = z.object({
  magnetOrUrl: z.string(),
  kind: z.string(),
  title: z.string().nullable(),
  year: z.number().nullable(),
  season: z.number().nullable(),
  episode: z.number().nullable(),
  tmdbId: z.number().nullable(),
  onlyFiles: z.array(z.number()).nullable(),
  detailsUrl: z.string().nullable(),
});
export type ManualAddBody = z.infer<typeof ManualAddBody>;

// ── Manual search ───────────────────────────────────────────────────────────

/** One release from a free-text manual indexer search. `guid` is an opaque
 * release id. */
export const ManualReleaseView = z.object({
  title: z.string(),
  guid: z.string(),
  indexerName: z.string(),
  downloadUrl: z.string().nullable(),
  sizeBytes: z.number().nullable(),
  seeders: z.number().nullable(),
  leechers: z.number().nullable(),
  publishedAt: z.string().nullable(),
  resolution: z.string().nullable(),
  codec: z.string().nullable(),
  source: z.string().nullable(),
  parsedTitle: z.string(),
  year: z.number().nullable(),
  season: z.number().nullable(),
  episode: z.number().nullable(),
  fullSeason: z.boolean(),
  detailsUrl: z.string().nullable(),
});
export type ManualReleaseView = z.infer<typeof ManualReleaseView>;

/** `POST /api/admin/acquisition/search` body. */
export const ManualSearchBody = z.object({
  query: z.string(),
});
export type ManualSearchBody = z.infer<typeof ManualSearchBody>;

/** `POST /api/admin/acquisition/search`. */
export const ManualSearchView = z.object({
  releases: z.array(ManualReleaseView),
  indexerErrors: z.array(z.string()),
});
export type ManualSearchView = z.infer<typeof ManualSearchView>;

// ── Interactive (scored) search ─────────────────────────────────────────────

/** One score-explanation line. */
export const ScoreLineView = z.object({
  rule: z.string(),
  delta: z.number(),
  note: z.string(),
});
export type ScoreLineView = z.infer<typeof ScoreLineView>;

/** One scored release from an interactive search. `guid` is an opaque release
 * id; `target` is an open string (`movie` | `episode` | `season`). */
export const ScoredReleaseView = z.object({
  title: z.string(),
  guid: z.string(),
  indexerId: IndexerId,
  indexerName: z.string(),
  sizeBytes: z.number().nullable(),
  seeders: z.number().nullable(),
  leechers: z.number().nullable(),
  publishedAt: z.string().nullable(),
  target: z.string(),
  season: z.number().nullable(),
  episodes: z.array(z.number()).nullable(),
  score: z.number().nullable(),
  breakdown: z.array(ScoreLineView),
  rejected: z.string().nullable(),
  grabbable: z.boolean(),
  detailsUrl: z.string().nullable(),
});
export type ScoredReleaseView = z.infer<typeof ScoredReleaseView>;

/** `GET /api/requests/:id/search`. */
export const InteractiveSearchView = z.object({
  releases: z.array(ScoredReleaseView),
  indexerErrors: z.array(z.string()),
});
export type InteractiveSearchView = z.infer<typeof InteractiveSearchView>;

// ── Indexers ────────────────────────────────────────────────────────────────

/** One configured Torznab indexer (API key write-only). `categories` are raw
 * Torznab category numbers. */
export const IndexerView = z.object({
  id: IndexerId,
  name: z.string(),
  url: z.string(),
  hasApiKey: z.boolean(),
  categories: z.array(z.number()),
  enabled: z.boolean(),
  priority: z.number(),
  /** `torznab` (external Jackett/Prowlarr) or `builtin` (native Cardigann). */
  kind: z.string(),
  /** Cardigann definition id (built-in indexers only). */
  definitionId: z.string().nullable(),
  /** Names of settings that currently hold a value (secrets never returned). */
  configuredSettings: z.array(z.string()),
  lastOkAt: z.number().nullable(),
  lastError: z.string().nullable(),
  createdAt: z.number(),
});
export type IndexerView = z.infer<typeof IndexerView>;

/** `GET /api/admin/indexers`. */
export const IndexersView = z.object({
  indexers: z.array(IndexerView),
});
export type IndexersView = z.infer<typeof IndexersView>;

/** `POST /api/admin/indexers/:id/test` result (a `t=caps` round-trip). */
export const IndexerTestResult = z.object({
  ok: z.boolean(),
  latencyMs: z.number(),
  serverTitle: z.string().nullable(),
  supportsTmdb: z.boolean(),
  error: z.string().nullable(),
});
export type IndexerTestResult = z.infer<typeof IndexerTestResult>;

/** Create/update body for an indexer (all fields optional patch; omitted
 * `apiKey` keeps the stored secret). */
export const SaveIndexerBody = z.object({
  name: z.string().nullable(),
  url: z.string().nullable(),
  apiKey: z.string().nullable(),
  categories: z.array(z.number()).nullable(),
  enabled: z.boolean().nullable(),
  priority: z.number().nullable(),
  /** `builtin` to create a native Cardigann indexer (default `torznab`). */
  kind: z.string().nullable().optional(),
  /** Cardigann definition id (built-in create). */
  definitionId: z.string().nullable().optional(),
  /** Per-indexer settings (credentials + toggles). */
  settings: z.record(z.string(), z.string()).nullable().optional(),
});
export type SaveIndexerBody = z.infer<typeof SaveIndexerBody>;

// ── Built-in definition catalog ─────────────────────────────────────────────

/** One Cardigann definition in the browse list. */
export const IndexerDefinitionView = z.object({
  id: z.string(),
  name: z.string(),
  kind: z.string(),
  description: z.string(),
  links: z.array(z.string()),
});
export type IndexerDefinitionView = z.infer<typeof IndexerDefinitionView>;

/** `GET /api/admin/indexers/definitions`. */
export const IndexerDefinitionsView = z.object({
  definitions: z.array(IndexerDefinitionView),
  synced: z.boolean(),
});
export type IndexerDefinitionsView = z.infer<typeof IndexerDefinitionsView>;

/** One configurable setting of a definition (for the add form). */
export const IndexerDefinitionSettingView = z.object({
  name: z.string(),
  kind: z.string(),
  label: z.string(),
  default: z.string().nullable(),
  /** For `select`: ordered [value, label] pairs. */
  options: z.array(z.tuple([z.string(), z.string()])),
});
export type IndexerDefinitionSettingView = z.infer<typeof IndexerDefinitionSettingView>;

/** `GET /api/admin/indexers/definitions/:id`. */
export const IndexerDefinitionDetailView = z.object({
  id: z.string(),
  name: z.string(),
  kind: z.string(),
  description: z.string(),
  links: z.array(z.string()),
  settings: z.array(IndexerDefinitionSettingView),
});
export type IndexerDefinitionDetailView = z.infer<typeof IndexerDefinitionDetailView>;

/** `POST /api/admin/indexers/definitions/sync` result. */
export const SyncDefinitionsResult = z.object({
  count: z.number(),
  version: z.string(),
});
export type SyncDefinitionsResult = z.infer<typeof SyncDefinitionsResult>;
