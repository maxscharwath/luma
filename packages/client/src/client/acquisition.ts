// Admin acquisition config: Torznab indexers (Jackett / Prowlarr), download
// clients (embedded engine + Transmission / qBittorrent) and the downloads
// queue.

import type {
  ClientTestResult,
  DownloadClientsView,
  DownloadClientView,
  DownloadsView,
  IndexersView,
  IndexerTestResult,
  IndexerView,
  ManualAddBody,
  ManualSearchView,
  SaveDownloadClientBody,
  SaveIndexerBody,
  SaveVpnBody,
  TorrentAnalysis,
  VpnAdminView,
  VpnTestResult,
} from '../types';
import type { RequestContext } from './base';

const JSON_HEADERS = { 'content-type': 'application/json' };

export function adminIndexers(ctx: RequestContext): Promise<IndexersView> {
  return ctx.json<IndexersView>('/admin/indexers');
}

export function createIndexer(ctx: RequestContext, body: SaveIndexerBody): Promise<IndexerView> {
  return ctx.json<IndexerView>('/admin/indexers', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

/** Partial update; omitted fields keep their values (an omitted apiKey keeps
 * the stored secret). */
export function updateIndexer(
  ctx: RequestContext,
  id: string,
  body: SaveIndexerBody,
): Promise<IndexerView> {
  return ctx.json<IndexerView>(`/admin/indexers/${encodeURIComponent(id)}`, {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

export function deleteIndexer(ctx: RequestContext, id: string): Promise<void> {
  return ctx.json<void>(`/admin/indexers/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

/** Live t=caps round-trip: latency, server title, TMDB id support. */
export function testIndexer(ctx: RequestContext, id: string): Promise<IndexerTestResult> {
  return ctx.json<IndexerTestResult>(`/admin/indexers/${encodeURIComponent(id)}/test`, {
    method: 'POST',
  });
}

// ----- download clients ---------------------------------------------------------

export function adminDownloadClients(ctx: RequestContext): Promise<DownloadClientsView> {
  return ctx.json<DownloadClientsView>('/admin/download-clients');
}

export function createDownloadClient(
  ctx: RequestContext,
  body: SaveDownloadClientBody,
): Promise<DownloadClientView> {
  return ctx.json<DownloadClientView>('/admin/download-clients', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

export function updateDownloadClient(
  ctx: RequestContext,
  id: string,
  body: SaveDownloadClientBody,
): Promise<DownloadClientView> {
  return ctx.json<DownloadClientView>(`/admin/download-clients/${encodeURIComponent(id)}`, {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

export function deleteDownloadClient(ctx: RequestContext, id: string): Promise<void> {
  return ctx.json<void>(`/admin/download-clients/${encodeURIComponent(id)}`, { method: 'DELETE' });
}

export function testDownloadClient(ctx: RequestContext, id: string): Promise<ClientTestResult> {
  return ctx.json<ClientTestResult>(`/admin/download-clients/${encodeURIComponent(id)}/test`, {
    method: 'POST',
  });
}

// ----- downloads queue ------------------------------------------------------------

export function adminDownloads(ctx: RequestContext): Promise<DownloadsView> {
  return ctx.json<DownloadsView>('/admin/downloads');
}

export function pauseDownload(ctx: RequestContext, id: string): Promise<void> {
  return ctx.json<void>(`/admin/downloads/${encodeURIComponent(id)}/pause`, { method: 'POST' });
}

export function resumeDownload(ctx: RequestContext, id: string): Promise<void> {
  return ctx.json<void>(`/admin/downloads/${encodeURIComponent(id)}/resume`, { method: 'POST' });
}

/** Re-attempt a failed grab (re-adds the torrent in the background). */
export function retryDownload(ctx: RequestContext, id: string): Promise<void> {
  return ctx.json<void>(`/admin/downloads/${encodeURIComponent(id)}/retry`, { method: 'POST' });
}

/** Force a tracker/DHT re-announce ("ask more peers") for one download. */
export function reannounceDownload(ctx: RequestContext, id: string): Promise<void> {
  return ctx.json<void>(`/admin/downloads/${encodeURIComponent(id)}/reannounce`, {
    method: 'POST',
  });
}

export function removeDownload(
  ctx: RequestContext,
  id: string,
  opts?: { deleteData?: boolean },
): Promise<void> {
  const qs = opts?.deleteData ? '?deleteData=true' : '';
  return ctx.json<void>(`/admin/downloads/${encodeURIComponent(id)}${qs}`, { method: 'DELETE' });
}

/** Pause every active LUMA download (foreign torrents in a shared client are
 * left untouched). Returns how many were paused. */
export function pauseAllDownloads(ctx: RequestContext): Promise<{ count: number }> {
  return ctx.json<{ count: number }>('/admin/downloads/pause-all', { method: 'POST' });
}

/** Resume every LUMA download we previously paused. */
export function resumeAllDownloads(ctx: RequestContext): Promise<{ count: number }> {
  return ctx.json<{ count: number }>('/admin/downloads/resume-all', { method: 'POST' });
}

/** Force a tracker/DHT re-announce ("ask more peers") on every active download. */
export function reannounceDownloads(ctx: RequestContext): Promise<{ count: number }> {
  return ctx.json<{ count: number }>('/admin/downloads/reannounce', { method: 'POST' });
}

/** Free-text manual indexer search (admin picks a release to grab). */
export function manualSearch(ctx: RequestContext, query: string): Promise<ManualSearchView> {
  return ctx.json<ManualSearchView>('/admin/downloads/search', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ query }),
  });
}

/** Fetch a torrent's file list (metadata only, no download) + what it holds,
 * so the admin can pick episodes / confirm the entity before grabbing. */
export function analyzeTorrent(ctx: RequestContext, magnetOrUrl: string): Promise<TorrentAnalysis> {
  return ctx.json<TorrentAnalysis>('/admin/downloads/analyze', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify({ magnetOrUrl }),
  });
}

/** Grab a pasted magnet / .torrent URL (or a manual-search result) and import
 * it as `kind` into the right library. */
export function manualAdd(ctx: RequestContext, body: ManualAddBody): Promise<{ id: string }> {
  return ctx.json<{ id: string }>('/admin/downloads/add', {
    method: 'POST',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

// ----- VPN (managed WireGuard bridge, Proton-friendly) ----------------------------

export function adminVpn(ctx: RequestContext): Promise<VpnAdminView> {
  return ctx.json<VpnAdminView>('/admin/vpn');
}

/** Store the WireGuard config (write-only; "" removes it) and restart the
 * bridge + embedded engine. */
export function saveVpn(
  ctx: RequestContext,
  body: SaveVpnBody,
): Promise<{ wgConfigured: boolean }> {
  return ctx.json<{ wgConfigured: boolean }>('/admin/vpn', {
    method: 'PUT',
    headers: JSON_HEADERS,
    body: JSON.stringify(body),
  });
}

/** Live seal probe: exit IP through the proxy vs direct. */
export function testVpn(ctx: RequestContext): Promise<VpnTestResult> {
  return ctx.json<VpnTestResult>('/admin/vpn/test', { method: 'POST' });
}
