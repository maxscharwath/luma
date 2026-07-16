// Media requests: submit (Overseerr-style ask for a title), track your own,
// and moderate the queue (approve / deny / interactive release search) with
// `requests.manage`.

import type {
  CalendarEntry,
  CreateRequestBody,
  GrabBody,
  InteractiveSearchView,
  MediaRequest,
  RequestsView,
} from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Own requests, or everyone's for a `requests.manage` holder. Pass
 * `mine: true` to force own-only (the user-facing "Mes demandes" page). */
export function listRequests(
  ctx: RequestContext,
  opts?: { mine?: boolean },
): Promise<RequestsView> {
  const qs = opts?.mine ? '?mine=true' : '';
  return ctx.json<RequestsView>(`/requests${qs}`);
}

/** The "coming soon" calendar: upcoming, not-yet-available releases (a movie's
 * availability date + a show episode's air date), ascending by date. `mine: true`
 * forces own-only (the user-facing page); a manager otherwise sees everyone's. */
export function getCalendar(
  ctx: RequestContext,
  opts?: { mine?: boolean },
): Promise<CalendarEntry[]> {
  const qs = opts?.mine ? '?mine=true' : '';
  return ctx.json<CalendarEntry[]>(`/requests/calendar${qs}`);
}

/** The "missing / wanted" list: aired/released items still not on disk (the
 * inverse of the calendar), for the Wanted view. `mine: true` forces own-only. */
export function getMissing(
  ctx: RequestContext,
  opts?: { mine?: boolean },
): Promise<CalendarEntry[]> {
  const qs = opts?.mine ? '?mine=true' : '';
  return ctx.json<CalendarEntry[]>(`/requests/missing${qs}`);
}

/** "Search all missing" (requests.manage): kick the acquisition search pass now,
 * which auto-grabs the best release for every aired-but-open item. Returns the
 * job run id. */
export function searchAllMissing(ctx: RequestContext): Promise<{ runId: string }> {
  return ctx.json<{ runId: string }>('/requests/search-missing', { method: 'POST' });
}

/** Per-title "ask to watch" (requests.manage): search this one request and grab
 * the best accepted release. Slow (a live indexer sweep). */
export function autoSearchRequest(
  ctx: RequestContext,
  id: string,
): Promise<{ grabbed: boolean; title?: string }> {
  return ctx.json<{ grabbed: boolean; title?: string }>(
    `/requests/${encodeURIComponent(id)}/auto-search`,
    { method: 'POST' },
  );
}

/** Submit a request. A second ask for the same title merges into the open one
 * (a show ask can widen its season subset). */
export function createRequest(ctx: RequestContext, body: CreateRequestBody): Promise<MediaRequest> {
  return ctx.json<MediaRequest>('/requests', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

/** Withdraw an own pending request, or (as a manager) delete any request. */
export function deleteRequest(ctx: RequestContext, id: string): Promise<void> {
  return ctx.json<void>(`/requests/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** Approve (requests.manage): materializes the wanted list + kicks the search. */
export function approveRequest(ctx: RequestContext, id: string): Promise<MediaRequest> {
  return ctx.json<MediaRequest>(`/requests/${encodeURIComponent(id)}/approve`, { method: 'POST' });
}

/** Deny (requests.manage), with an optional reason shown to the requester. */
export function denyRequest(ctx: RequestContext, id: string, note?: string): Promise<MediaRequest> {
  return ctx.json<MediaRequest>(`/requests/${encodeURIComponent(id)}/deny`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(note ? { note } : {}),
  });
}

/** Interactive search (requests.manage): live sweep of every enabled indexer
 * for this request, returning scored releases + rejects with reasons. Slow
 * (Torznab round-trips); show a spinner. */
export function searchReleases(ctx: RequestContext, id: string): Promise<InteractiveSearchView> {
  return ctx.json<InteractiveSearchView>(`/requests/${encodeURIComponent(id)}/search`);
}

/** Manually grab one release from the last interactive search. */
export function grabRelease(ctx: RequestContext, id: string, body: GrabBody): Promise<void> {
  return ctx.json<void>(`/requests/${encodeURIComponent(id)}/grab`, {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}
