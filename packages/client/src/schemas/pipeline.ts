// Runtime schemas for the pipeline / jobs / naming / VPN / LLM domain.
//
// Mirrors the ts-rs generated wire types (the Rust structs are the single source
// of truth) with runtime validation and branded ids. The `true satisfies
// SameKeys<…>` lines are compile-time drift guards: change a Rust struct + rerun
// `gen:types` and the build breaks here until the schema is updated. See
// `./accounts` for the template and `./drift` for how the guards work.

import { z } from 'zod';
import { EpStats } from './admin';
import { JobRunId } from './ids';
import { Category, Treatment } from './media';

// ── Jobs ────────────────────────────────────────────────────────────────────

/** One recorded execution of a job. */
export const JobRun = z.object({
  id: JobRunId,
  jobKey: z.string(),
  trigger: z.string(),
  status: z.string(),
  startedAt: z.number(),
  finishedAt: z.number().nullable(),
  durationMs: z.number().nullable(),
  progressDone: z.number().nullable(),
  progressTotal: z.number().nullable(),
  error: z.string().nullable(),
});
export type JobRun = z.infer<typeof JobRun>;

/** A job's definition + current state, as listed in the admin console. */
export const JobInfo = z.object({
  key: z.string(),
  name: z.string(),
  description: z.string(),
  category: Category,
  schedule: z.string().nullable(),
  defaultSchedule: z.string().nullable(),
  customized: z.boolean(),
  enabled: z.boolean(),
  running: z.boolean(),
  runId: JobRunId.nullable(),
  progressDone: z.number().nullable(),
  progressTotal: z.number().nullable(),
  nextRunAt: z.number().nullable(),
  lastRun: JobRun.nullable(),
});
export type JobInfo = z.infer<typeof JobInfo>;

/** One persisted log line of a run. */
export const JobLog = z.object({
  ts: z.number(),
  level: z.string(),
  message: z.string(),
});
export type JobLog = z.infer<typeof JobLog>;

/** `GET /api/admin/jobs/:key` a job plus its recent run history. */
export const JobDetail = z.object({
  info: JobInfo,
  runs: z.array(JobRun),
});
export type JobDetail = z.infer<typeof JobDetail>;

/** `GET /api/admin/jobs`. */
export const JobsView = z.object({
  jobs: z.array(JobInfo),
});
export type JobsView = z.infer<typeof JobsView>;

// ── Pipeline ──────────────────────────────────────────────────────────────

/** Health counters for one pipeline stage, aggregated from the ledger. */
export const StageStat = z.object({
  stage: z.string(),
  key: z.string(),
  subjectKind: z.string(),
  pending: z.number(),
  running: z.number(),
  done: z.number(),
  failed: z.number(),
  blocked: z.number(),
});
export type StageStat = z.infer<typeof StageStat>;

/** Status tally over ALL elements (unfiltered), for the filter chips + header. */
export const ElementCounts = z.object({
  total: z.number(),
  ok: z.number(),
  pending: z.number(),
  running: z.number(),
  failed: z.number(),
  film: z.number(),
  series: z.number(),
  episode: z.number(),
});
export type ElementCounts = z.infer<typeof ElementCounts>;

/** Every treatment that applies to an element + whether it has been done. */
export const ElementProcessing = z.object({
  treatments: z.array(Treatment),
});
export type ElementProcessing = z.infer<typeof ElementProcessing>;

/** One catalog element (film / series / episode) with per-treatment status. */
export const ElementRow = z.object({
  id: z.string(),
  kind: z.string(),
  title: z.string(),
  poster: z.string().nullable(),
  year: z.number().nullable(),
  genre: z.string().nullable(),
  durationMs: z.number().nullable(),
  seasonCount: z.number().nullable(),
  treatments: z.array(Treatment),
  overall: z.string(),
  epStats: EpStats.nullish(),
});
export type ElementRow = z.infer<typeof ElementRow>;

/** `GET /api/admin/pipeline/elements`: a filtered, paginated page of the catalog. */
export const PipelineElements = z.object({
  total: z.number(),
  page: z.number(),
  pages: z.number(),
  counts: ElementCounts,
  elements: z.array(ElementRow),
});
export type PipelineElements = z.infer<typeof PipelineElements>;

/** One failed (or otherwise notable) ledger row, for the stage drill-down. */
export const PipelineTaskView = z.object({
  stage: z.string(),
  subjectKind: z.string(),
  subjectId: z.string(),
  title: z.string(),
  status: z.string(),
  attempts: z.number(),
  error: z.string().nullable(),
  finishedAt: z.number().nullable(),
});
export type PipelineTaskView = z.infer<typeof PipelineTaskView>;

/** `GET /api/admin/pipeline`: every stage's health, in DAG order. */
export const PipelineView = z.object({
  stages: z.array(StageStat),
  paused: z.boolean(),
});
export type PipelineView = z.infer<typeof PipelineView>;

// ── Naming / organize ───────────────────────────────────────────────────────

/** Example rendered names for the live preview. */
export const SampleNames = z.object({
  movie: z.string(),
  episode: z.string(),
});
export type SampleNames = z.infer<typeof SampleNames>;

/** The five naming templates plus the global case transform. */
export const NamingTemplatesView = z.object({
  movieFolder: z.string(),
  movieFile: z.string(),
  seriesFolder: z.string(),
  seasonFolder: z.string(),
  episodeFile: z.string(),
  case: z.string(),
});
export type NamingTemplatesView = z.infer<typeof NamingTemplatesView>;

/** `GET /api/admin/organize/naming` current templates + a rendered sample. */
export const NamingView = z.object({
  templates: NamingTemplatesView,
  sample: SampleNames,
});
export type NamingView = z.infer<typeof NamingView>;

/** One file the rename tool would move. */
export const OrganizeMove = z.object({
  title: z.string(),
  kind: z.string(),
  from: z.string(),
  to: z.string(),
});
export type OrganizeMove = z.infer<typeof OrganizeMove>;

/** `GET /api/admin/organize/preview`. */
export const OrganizePlan = z.object({
  moves: z.array(OrganizeMove),
  totalFiles: z.number(),
  matching: z.number(),
});
export type OrganizePlan = z.infer<typeof OrganizePlan>;

/** `POST /api/admin/organize/apply` result. */
export const OrganizeResult = z.object({
  moved: z.number(),
  failed: z.number(),
  errors: z.array(z.string()),
});
export type OrganizeResult = z.infer<typeof OrganizeResult>;

/** `POST /api/admin/acquisition/analyze` body. */
export const AnalyzeBody = z.object({
  magnetOrUrl: z.string(),
});
export type AnalyzeBody = z.infer<typeof AnalyzeBody>;

// ── VPN ───────────────────────────────────────────────────────────────────

/** The kill switch's view of the tunnel. */
export const VpnStatusView = z.object({
  connected: z.boolean(),
  exitIp: z.string().nullable(),
  paused: z.boolean(),
});
export type VpnStatusView = z.infer<typeof VpnStatusView>;

/** `POST /api/admin/vpn/test` a live probe through (and around) the proxy. */
export const VpnTestResult = z.object({
  sealed: z.boolean(),
  proxiedIp: z.string().nullable(),
  directIp: z.string().nullable(),
  error: z.string().nullable(),
});
export type VpnTestResult = z.infer<typeof VpnTestResult>;

/** `GET /api/admin/vpn` the VPN configuration card's state. */
export const VpnAdminView = z.object({
  wgConfigured: z.boolean(),
  bridgeRunning: z.boolean(),
  localPort: z.number(),
  status: VpnStatusView.nullable(),
});
export type VpnAdminView = z.infer<typeof VpnAdminView>;

/** `PUT /api/admin/vpn` body. `wgConfig` is write-only. */
export const SaveVpnBody = z.object({
  wgConfig: z.string().nullable(),
  localPort: z.number().nullable(),
});
export type SaveVpnBody = z.infer<typeof SaveVpnBody>;

// ── LLM ───────────────────────────────────────────────────────────────────

/** One configured provider as shown to the admin (API key never returned). */
export const LlmProviderView = z.object({
  id: z.string(),
  name: z.string(),
  provider: z.string(),
  baseUrl: z.string(),
  model: z.string(),
  hasApiKey: z.boolean(),
  temperature: z.number(),
  maxTokens: z.number(),
  reasoning: z.boolean(),
});
export type LlmProviderView = z.infer<typeof LlmProviderView>;

/** `GET /api/admin/llm` the multi-provider LLM configuration. */
export const LlmAdminConfig = z.object({
  enabled: z.boolean(),
  defaultId: z.string(),
  providers: z.array(LlmProviderView),
});
export type LlmAdminConfig = z.infer<typeof LlmAdminConfig>;
