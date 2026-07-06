// Media requests: submit (Overseerr-style ask for a title), track your own,
// and moderate the queue (approve / deny / interactive release search) with
// `requests.manage`.

import type {
  CreateRequestBody,
  GrabBody,
  InteractiveSearchView,
  MediaRequest,
  RequestsView,
} from '../generated';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

/** Own requests, or everyone's for a `requests.manage` holder. Pass
 * `mine: true` to force own-only (the user-facing "Mes demandes" page). */
export function listRequests(ctx: RequestContext, opts?: { mine?: boolean }): Promise<RequestsView> {
  const qs = opts?.mine ? '?mine=true' : '';
  return ctx.json<RequestsView>(`/requests${qs}`);
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
