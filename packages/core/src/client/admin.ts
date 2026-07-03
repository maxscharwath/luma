// Admin console: server identity, live sessions, metrics/storage, users,
// settings and watch stats.

import type {
  AdminOverview,
  AdminUsers,
  HistoryStats,
  JobDetail,
  JobLog,
  ElementProcessing,
  JobsView,
  PipelineElements,
  LlmAdminConfig,
  MetricsSnapshot,
  Permission,
  PipelineTaskView,
  PipelineView,
  PlaybackSession,
  ServerInfo,
  SettingsView,
  StorageInfo,
  TopUser,
} from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Server identity + uptime (requires an admin capability). */
export function adminServer(ctx: RequestContext): Promise<ServerInfo> {
  return ctx.json<ServerInfo>('/admin/server');
}

/** Live playback sessions for the dashboard. */
export function adminSessions(ctx: RequestContext): Promise<{ sessions: PlaybackSession[] }> {
  return ctx.json<{ sessions: PlaybackSession[] }>('/admin/sessions');
}

/** Terminate a live playback session; the owning client stops and shows
 * `message` (empty → the client's localized default). */
export async function terminateSession(
  ctx: RequestContext,
  id: string,
  message?: string,
): Promise<void> {
  await ctx.json<void>(`/admin/sessions/${encodeURIComponent(id)}/stop`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ message: message ?? '' }),
  });
}

/** CPU / RAM / bandwidth snapshot + history (poll for live charts). */
export function adminMetrics(ctx: RequestContext): Promise<MetricsSnapshot> {
  return ctx.json<MetricsSnapshot>('/admin/metrics');
}

/** Volumes, totals and cache usage. */
export function adminStorage(ctx: RequestContext): Promise<StorageInfo> {
  return ctx.json<StorageInfo>('/admin/storage');
}

/** Wipe transcode + image caches (requires `settings.manage`). */
export function clearCache(ctx: RequestContext): Promise<{ freedBytes: number }> {
  return ctx.json<{ freedBytes: number }>('/admin/cache/clear', { method: 'POST' });
}

/**
 * Drop every resolved TMDB metadata (DB JSON, season casts, embeddings) and the
 * in-memory lookup cache so the next enrichment re-fetches from scratch. Returns
 * how many movies/videos and shows were cleared (requires `settings.manage`).
 */
export function resetMetadata(ctx: RequestContext): Promise<{ items: number; shows: number }> {
  return ctx.json<{ items: number; shows: number }>('/admin/cache/reset-metadata', {
    method: 'POST',
  });
}

// ----- library folder browser -------------------------------------------------

/** One directory entry returned by the server-side folder browser. */
export interface AdminFsEntry {
  name: string;
  path: string;
}

/** A directory listing for the library folder picker: the current absolute
 *  `path`, its `parent` (null at a root), and its immediate subdirectories. */
export interface AdminFsList {
  path: string;
  parent: string | null;
  entries: AdminFsEntry[];
}

/** Browse server-side directories for the library folder picker. An empty/absent
 *  `path` returns the roots (NAS volumes, or `/` in dev). Requires an admin
 *  capability. */
export function adminBrowseFolders(ctx: RequestContext, path?: string): Promise<AdminFsList> {
  const qs = path ? `?path=${encodeURIComponent(path)}` : '';
  return ctx.json<AdminFsList>(`/admin/libraries/browse${qs}`);
}

/** Full member list (requires `users.manage`). */
export function adminUsers(ctx: RequestContext): Promise<AdminUsers> {
  return ctx.json<AdminUsers>('/admin/users');
}

/** Update a user's permissions and/or username. */
export async function updateUser(
  ctx: RequestContext,
  id: string,
  patch: { permissions?: Permission[]; username?: string },
): Promise<void> {
  await ctx.json<void>(`/admin/users/${encodeURIComponent(id)}`, {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify(patch),
  });
}

/** Delete a user account. */
export async function deleteUser(ctx: RequestContext, id: string): Promise<void> {
  await ctx.json<void>(`/admin/users/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** Grouped settings schema + current values for one view. */
export function adminSettings(ctx: RequestContext, view: string): Promise<SettingsView> {
  return ctx.json<SettingsView>(`/admin/settings?view=${encodeURIComponent(view)}`);
}

/** Persist a settings patch → the keys actually written. */
export function updateSettings(
  ctx: RequestContext,
  patch: Record<string, unknown>,
): Promise<{ updated: string[] }> {
  return ctx.json<{ updated: string[] }>('/admin/settings', {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify(patch),
  });
}

// ----- portable backup --------------------------------------------------------

/** Per-table row counts written by an import, plus whether a re-scan was kicked. */
export interface BackupImportResult {
  imported: Record<string, number>;
  rescanStarted: boolean;
}

/** Options for restoring a backup. */
export interface BackupImportOptions {
  /** Password for an encrypted (`.luma`) backup. */
  password?: string;
  /** Wipe this server's portable tables before importing (clean A→B clone). */
  reset?: boolean;
}

/** Hex-encode a UTF-8 string so an arbitrary password survives an HTTP header. */
function hexUtf8(s: string): string {
  return Array.from(new TextEncoder().encode(s))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
}

/** Download a portable backup (accounts, settings, history, resume positions,
 *  invites, cron overrides, custom avatars) as a `Blob`. A `password` encrypts it
 *  (`.luma`), else a plain `.zip`. Requires `settings.manage`. */
export function exportBackup(ctx: RequestContext, password?: string): Promise<Blob> {
  return ctx.blob(
    '/admin/backup/export',
    password ? { headers: { 'x-backup-password': hexUtf8(password) } } : undefined,
  );
}

/** Restore a backup file (`.zip`/`.luma`/legacy `.json`), then trigger a re-scan
 *  so the catalogue regenerates with matching item IDs. */
export function importBackup(
  ctx: RequestContext,
  file: Blob,
  opts?: BackupImportOptions,
): Promise<BackupImportResult> {
  const headers: Record<string, string> = { ...JSON_HEADERS };
  if (opts?.password) headers['x-backup-password'] = hexUtf8(opts.password);
  if (opts?.reset) headers['x-backup-reset'] = '1';
  return ctx.json<BackupImportResult>('/admin/backup/import', {
    method: 'POST',
    headers,
    body: file,
  });
}

/** Per-user watch aggregates over the last `days` (default 7). */
export function topUsers(ctx: RequestContext, days = 7): Promise<{ users: TopUser[] }> {
  return ctx.json<{ users: TopUser[] }>(`/admin/stats/top-users?days=${days}`);
}

/** Weekly films-vs-TV watch buckets over the last `days` (default 28). */
export function playHistory(ctx: RequestContext, days = 28): Promise<HistoryStats> {
  return ctx.json<HistoryStats>(`/admin/stats/history?days=${days}`);
}

/** Top-line counts for the users page. */
export function adminOverview(ctx: RequestContext): Promise<AdminOverview> {
  return ctx.json<AdminOverview>('/admin/stats/overview');
}

// ----- background jobs / scheduler --------------------------------------------

/** Every background job with its schedule, last run and next fire. */
export function adminJobs(ctx: RequestContext): Promise<JobsView> {
  return ctx.json<JobsView>('/admin/jobs');
}

/** One job plus its recent run history. */
export function adminJob(ctx: RequestContext, key: string): Promise<JobDetail> {
  return ctx.json<JobDetail>(`/admin/jobs/${encodeURIComponent(key)}`);
}

/** Trigger a job now (manual). Resolves with the new run id. */
export function runJob(ctx: RequestContext, key: string): Promise<{ runId: string }> {
  return ctx.json<{ runId: string }>(`/admin/jobs/${encodeURIComponent(key)}/run`, {
    method: 'POST',
  });
}

/** Request cancellation of a job's current run. */
export function cancelJob(ctx: RequestContext, key: string): Promise<{ cancelled: boolean }> {
  return ctx.json<{ cancelled: boolean }>(`/admin/jobs/${encodeURIComponent(key)}/cancel`, {
    method: 'POST',
  });
}

/** Update a job's cron schedule (`null` clears it) and/or enabled flag. */
export async function updateJob(
  ctx: RequestContext,
  key: string,
  patch: { schedule?: string | null; enabled?: boolean },
): Promise<void> {
  await ctx.json<void>(`/admin/jobs/${encodeURIComponent(key)}`, {
    method: 'PATCH',
    headers: JSON_HEADERS,
    body: JSON.stringify(patch),
  });
}

/** The log lines of a specific run (chronological). */
export function jobRunLogs(ctx: RequestContext, runId: string): Promise<{ logs: JobLog[] }> {
  return ctx.json<{ logs: JobLog[] }>(`/admin/job-runs/${encodeURIComponent(runId)}/logs`);
}

// ----- per-element processing pipeline ----------------------------------------

/** Per-stage health counts (probe/metadata/storyboard/markers/…). */
export function adminPipeline(ctx: RequestContext): Promise<PipelineView> {
  return ctx.json<PipelineView>('/admin/pipeline');
}

/** A stage's failed tasks (newest first) for the drill-down. */
export function pipelineFailed(
  ctx: RequestContext,
  stage: string,
): Promise<{ tasks: PipelineTaskView[] }> {
  return ctx.json<{ tasks: PipelineTaskView[] }>(
    `/admin/pipeline/${encodeURIComponent(stage)}/failed`,
  );
}

/** Trigger a stage's drain now. */
export function runPipelineStage(ctx: RequestContext, stage: string): Promise<{ runId: string }> {
  return ctx.json<{ runId: string }>(`/admin/pipeline/${encodeURIComponent(stage)}/run`, {
    method: 'POST',
  });
}

/** Cancel a stage's running drain. */
export function cancelPipelineStage(
  ctx: RequestContext,
  stage: string,
): Promise<{ cancelled: boolean }> {
  return ctx.json<{ cancelled: boolean }>(`/admin/pipeline/${encodeURIComponent(stage)}/cancel`, {
    method: 'POST',
  });
}

/** Hold (paused=true) or release all pipeline stages. Returns the new state. */
export function pausePipeline(ctx: RequestContext, paused: boolean): Promise<{ paused: boolean }> {
  return ctx.json<{ paused: boolean }>('/admin/pipeline/pause', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ paused }),
  });
}

/** Reset all of a stage's failed tasks to pending. */
export function retryPipelineStage(
  ctx: RequestContext,
  stage: string,
): Promise<{ requeued: number }> {
  return ctx.json<{ requeued: number }>(`/admin/pipeline/${encodeURIComponent(stage)}/retry`, {
    method: 'POST',
  });
}

/** Force a full re-run of a stage (every non-running task back to pending). */
export function reprocessPipelineStage(
  ctx: RequestContext,
  stage: string,
): Promise<{ requeued: number }> {
  return ctx.json<{ requeued: number }>(`/admin/pipeline/${encodeURIComponent(stage)}/reprocess`, {
    method: 'POST',
  });
}

/** Reset one failed task to pending. */
export function retryPipelineTask(
  ctx: RequestContext,
  stage: string,
  subjectId: string,
): Promise<{ requeued: number }> {
  return ctx.json<{ requeued: number }>(`/admin/pipeline/${encodeURIComponent(stage)}/task/retry`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ subjectId }),
  });
}

/** The catalog as a filtered, paginated list of elements with per-treatment
 *  status + full-catalog counts (the element-centric pipeline dashboard). */
export function pipelineElements(
  ctx: RequestContext,
  params: { status?: string; kind?: string; q?: string; page?: number; limit?: number } = {},
): Promise<PipelineElements> {
  const p = new URLSearchParams();
  if (params.status) p.set('status', params.status);
  if (params.kind) p.set('kind', params.kind);
  if (params.q) p.set('q', params.q);
  if (params.page != null) p.set('page', String(params.page));
  if (params.limit != null) p.set('limit', String(params.limit));
  const qs = p.toString();
  return ctx.json<PipelineElements>(`/admin/pipeline/elements${qs ? `?${qs}` : ''}`);
}

/** Re-run one stage for one element (the drawer's per-treatment retry). */
export async function retryElementStage(
  ctx: RequestContext,
  kind: 'item' | 'show',
  id: string,
  stage: string,
): Promise<void> {
  await ctx.json<void>('/admin/pipeline/element/retry', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ kind, id, stage }),
  });
}

/** The treatments applied to one movie/episode and their status. */
export function itemProcessing(ctx: RequestContext, id: string): Promise<ElementProcessing> {
  return ctx.json<ElementProcessing>(`/admin/pipeline/item/${encodeURIComponent(id)}`);
}

/** The treatments applied to a whole series (aggregated across episodes). */
export function showProcessing(ctx: RequestContext, id: string): Promise<ElementProcessing> {
  return ctx.json<ElementProcessing>(`/admin/pipeline/show/${encodeURIComponent(id)}`);
}

/** Force one element (a movie/episode `item`, or a whole `show`) through every
 *  pipeline stage now: clears its artifacts, requeues its tasks, kicks the stages. */
export function reprocessSubject(
  ctx: RequestContext,
  kind: 'item' | 'show',
  id: string,
): Promise<{ subjects: number; stages: string[] }> {
  return ctx.json<{ subjects: number; stages: string[] }>('/admin/pipeline/subject/reprocess', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ kind, id }),
  });
}

// ----- AI / LLM configuration -------------------------------------------------

/** Probe values (the in-progress form); blank fields fall back to the saved
 *  provider identified by `id` (notably a masked API key). */
export interface LlmProbe {
  /** The provider being edited, so a blank key reuses that provider's stored one. */
  id?: string;
  provider?: string;
  baseUrl?: string;
  model?: string;
  apiKey?: string;
}

/** Current LLM config: all providers + the default id (keys never returned). */
export function adminLlm(ctx: RequestContext): Promise<LlmAdminConfig> {
  return ctx.json<LlmAdminConfig>('/admin/llm');
}

/** A provider as sent on save like the view but without `hasApiKey`, plus an
 *  optional `apiKey` (blank/omitted keeps the stored secret). */
export interface LlmProviderInput {
  id: string;
  name: string;
  provider: string;
  baseUrl: string;
  model: string;
  apiKey?: string;
  temperature: number;
  maxTokens: number;
  reasoning: boolean;
}

/** The full IA config to persist (PUT /admin/llm). The default is identified by
 *  **index** a not-yet-saved provider has no id yet (the server assigns one). */
export interface LlmSave {
  enabled: boolean;
  defaultIndex: number;
  providers: LlmProviderInput[];
}

/** Persist the provider list + default selection + enable flag. */
export function saveLlm(ctx: RequestContext, body: LlmSave): Promise<void> {
  return ctx.json<void>('/admin/llm', {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

/** List the models an endpoint advertises (for the model picker). */
export function llmModels(
  ctx: RequestContext,
  probe: LlmProbe,
): Promise<{ models: string[]; error?: string }> {
  return ctx.json('/admin/llm/models', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(probe),
  });
}

/** Probe a connection (trivial completion). Always resolves with `{ ok, message }`. */
export function testLlm(
  ctx: RequestContext,
  probe: LlmProbe,
): Promise<{ ok: boolean; message: string }> {
  return ctx.json('/admin/llm/test', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(probe),
  });
}

