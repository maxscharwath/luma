// Problem reports (the "signaler un probleme" flow): any user files one on a
// movie / show / episode; `reports.manage` holders triage the queue
// (resolve / dismiss / reopen / delete).

import type { CreateReportBody, Report, ReportsView } from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Filters for the admin triage queue. */
export interface ReportQuery {
  status?: string;
  category?: string;
  kind?: string;
  q?: string;
}

/** File a problem report. The server resolves + snapshots the subject title
 * (404 when the movie/show/episode is unknown). */
export function createReport(ctx: RequestContext, body: CreateReportBody): Promise<Report> {
  return ctx.json<Report>('/reports', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

/** The caller's own reports, newest-first. */
export function listMyReports(ctx: RequestContext): Promise<Report[]> {
  return ctx.json<Report[]>('/reports/mine');
}

/** The admin triage queue (`reports.manage`), filtered + with status tallies. */
export function adminReports(ctx: RequestContext, query?: ReportQuery): Promise<ReportsView> {
  const params = new URLSearchParams();
  if (query?.status) params.set('status', query.status);
  if (query?.category) params.set('category', query.category);
  if (query?.kind) params.set('kind', query.kind);
  if (query?.q) params.set('q', query.q);
  const qs = params.toString();
  return ctx.json<ReportsView>(`/admin/reports${qs ? `?${qs}` : ''}`);
}

/** Resolve a report (`reports.manage`). */
export function resolveReport(ctx: RequestContext, id: string): Promise<Report> {
  return ctx.json<Report>(`/admin/reports/${encodeURIComponent(id)}/resolve`, { method: 'POST' });
}

/** Dismiss a report as not actionable (`reports.manage`). */
export function dismissReport(ctx: RequestContext, id: string): Promise<Report> {
  return ctx.json<Report>(`/admin/reports/${encodeURIComponent(id)}/dismiss`, { method: 'POST' });
}

/** Reopen a resolved / dismissed report (`reports.manage`). */
export function reopenReport(ctx: RequestContext, id: string): Promise<Report> {
  return ctx.json<Report>(`/admin/reports/${encodeURIComponent(id)}/reopen`, { method: 'POST' });
}

/** Delete a report (`reports.manage`). */
export function deleteReport(ctx: RequestContext, id: string): Promise<void> {
  return ctx.json<void>(`/admin/reports/${encodeURIComponent(id)}`, { method: 'DELETE' });
}
