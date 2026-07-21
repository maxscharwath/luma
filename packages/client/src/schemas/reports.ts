// Runtime schemas for the problem-report domain (the "signaler un probleme"
// flow). Mirrors the Rust `reports` module. Users file a report on a movie / show
// / episode; `reports.manage` holders triage them. `Option` fields are
// `.nullable()` (the server always emits them, as `null` when unset).

import { z } from 'zod';
import { ReportId, UserId } from './ids';

/** What a report is filed against (movies + episodes are `items`; a show is its
 * own aggregate). */
export const ReportSubjectKind = z.enum(['movie', 'show', 'episode']);
export type ReportSubjectKind = z.infer<typeof ReportSubjectKind>;

/** The nature of the reported problem. `metadata` = a wrong fiche (title /
 * overview / poster / cast / bad match); `other` carries a free-text message. */
export const ReportCategory = z.enum(['metadata', 'video', 'audio', 'subtitles', 'other']);
export type ReportCategory = z.infer<typeof ReportCategory>;

/** A report's triage state. */
export const ReportStatus = z.enum(['open', 'resolved', 'dismissed']);
export type ReportStatus = z.infer<typeof ReportStatus>;

/** One problem report, as listed in the admin queue (reporter's name hydrated). */
export const Report = z.object({
  id: ReportId,
  subjectKind: ReportSubjectKind,
  /** Local catalog id (movie/episode item id, or show id) deep-links to the fiche. */
  subjectId: z.string(),
  /** Display title snapshotted at report time (survives a re-scan / deletion). */
  subjectTitle: z.string(),
  category: ReportCategory,
  message: z.string().nullable(),
  status: ReportStatus,
  reportedBy: UserId.nullable(),
  reportedByName: z.string().nullable(),
  resolvedBy: UserId.nullable(),
  resolvedAt: z.number().nullable(),
  createdAt: z.number(),
  updatedAt: z.number(),
});
export type Report = z.infer<typeof Report>;

/** Status tallies for the admin queue's filter chips. */
export const ReportCounts = z.object({
  total: z.number(),
  open: z.number(),
  resolved: z.number(),
  dismissed: z.number(),
});
export type ReportCounts = z.infer<typeof ReportCounts>;

/** `GET /api/admin/reports`. */
export const ReportsView = z.object({
  reports: z.array(Report),
  counts: ReportCounts,
});
export type ReportsView = z.infer<typeof ReportsView>;

/** `POST /api/reports` body. The server resolves + snapshots `subjectTitle`. */
export const CreateReportBody = z.object({
  subjectKind: ReportSubjectKind,
  subjectId: z.string(),
  category: ReportCategory,
  message: z.string().nullish(),
});
export type CreateReportBody = z.infer<typeof CreateReportBody>;
