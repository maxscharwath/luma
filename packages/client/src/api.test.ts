import { describe, expect, it, vi } from 'vitest';
import { KromaClient } from './api';
import { KromaApiError } from './client/base';
import { ItemId } from './schemas/ids';

// A recording fetch: captures every request (url + method + body + headers) and
// returns a configurable response. The default response is a 200 with `{}` so a
// delegating method's request is issued (and recorded) even when its response
// validation later rejects the promise - we only assert the request was made.
interface Recorded {
  url: string;
  method: string;
  body: unknown;
  headers: Headers;
}
function recordingFetch(
  responder?: (
    url: string,
    init?: RequestInit,
  ) => Partial<{
    ok: boolean;
    status: number;
    json: unknown;
    blob: Blob;
    text: string;
  }>,
): { fetch: typeof globalThis.fetch; calls: Recorded[] } {
  const calls: Recorded[] = [];
  const fetch = vi.fn(async (url: RequestInfo | URL, init?: RequestInit) => {
    calls.push({
      url: String(url),
      method: init?.method ?? 'GET',
      body: init?.body,
      headers: new Headers(init?.headers),
    });
    const r = responder?.(String(url), init) ?? {};
    return {
      ok: r.ok ?? true,
      status: r.status ?? 200,
      json: async () => r.json ?? {},
      blob: async () => r.blob ?? new Blob(),
      // Mirror a real Response: the text body is the serialized JSON unless a
      // responder sets `text` explicitly (e.g. to model an empty 2xx body).
      text: async () => r.text ?? JSON.stringify(r.json ?? {}),
    } as unknown as Response;
  }) as unknown as typeof globalThis.fetch;
  return { fetch, calls };
}

function makeClient(
  responder?: Parameters<typeof recordingFetch>[0],
  opts?: { authToken?: string; locale?: string },
) {
  const { fetch, calls } = recordingFetch(responder);
  const client = new KromaClient({
    baseUrl: 'http://kroma.test',
    fetch,
    authToken: opts?.authToken,
    locale: opts?.locale,
  });
  return { client, calls };
}

describe('KromaClient constructor: baseUrl normalization', () => {
  it('strips trailing slashes while preserving the scheme separator', () => {
    expect(new KromaClient({ baseUrl: 'http://nas:4040/' }).baseUrl).toBe('http://nas:4040');
    expect(new KromaClient({ baseUrl: 'http://nas:4040///' }).baseUrl).toBe('http://nas:4040');
    expect(new KromaClient({ baseUrl: 'http://nas:4040' }).baseUrl).toBe('http://nas:4040');
    expect(new KromaClient({ baseUrl: 'http://nas/lib/' }).baseUrl).toBe('http://nas/lib');
  });
});

describe('auth token, locale and hasAuth', () => {
  it('reports hasAuth only when a token is set', () => {
    expect(new KromaClient({ baseUrl: 'http://x' }).hasAuth).toBe(false);
    const c = new KromaClient({ baseUrl: 'http://x', authToken: 'tok' });
    expect(c.hasAuth).toBe(true);
    c.setAuthToken(undefined);
    expect(c.hasAuth).toBe(false);
    c.setAuthToken('again');
    expect(c.hasAuth).toBe(true);
  });

  it('sends the current bearer + Accept-Language on requests', async () => {
    const { client, calls } = makeClient(undefined, { authToken: 'tok', locale: 'fr' });
    await client.health();
    expect(calls[0]?.headers.get('Authorization')).toBe('Bearer tok');
    expect(calls[0]?.headers.get('Accept-Language')).toBe('fr');
  });

  it('setAuthToken / setLocale change what later requests carry', async () => {
    const { client, calls } = makeClient();
    client.setAuthToken('t2');
    client.setLocale('en');
    await client.health();
    expect(calls[0]?.headers.get('Authorization')).toBe('Bearer t2');
    expect(calls[0]?.headers.get('Accept-Language')).toBe('en');
  });

  it('omits auth/locale headers when neither is set', async () => {
    const { client, calls } = makeClient();
    await client.health();
    expect(calls[0]?.headers.get('Authorization')).toBeNull();
    expect(calls[0]?.headers.get('Accept-Language')).toBeNull();
  });
});

describe('silent refresh on 401 (json)', () => {
  // Fail the first request to a path with 401, then succeed on the retry.
  function refreshingFetch(failPathPart: string) {
    let hits = 0;
    return recordingFetch((url) => {
      if (url.includes(failPathPart)) {
        hits += 1;
        if (hits === 1) return { ok: false, status: 401, json: { error: 'expired' } };
      }
      return { ok: true, status: 200, json: {} };
    });
  }

  it('refreshes once from the handler then retries with the new token', async () => {
    const { fetch, calls } = refreshingFetch('/home');
    const client = new KromaClient({ baseUrl: 'http://kroma.test', fetch, authToken: 'old' });
    const refresh = vi.fn(async () => 'fresh');
    client.setRefreshHandler(refresh);

    await expect(client.home()).resolves.toEqual({});
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(calls).toHaveLength(2);
    // The retry carries the refreshed bearer.
    expect(calls[1]?.headers.get('Authorization')).toBe('Bearer fresh');
  });

  it('propagates the 401 when there is no refresh handler', async () => {
    const { fetch } = refreshingFetch('/home');
    const client = new KromaClient({ baseUrl: 'http://kroma.test', fetch, authToken: 'old' });
    await expect(client.home()).rejects.toBeInstanceOf(KromaApiError);
  });

  it('propagates the 401 when the handler cannot refresh (undefined)', async () => {
    const { fetch, calls } = refreshingFetch('/home');
    const client = new KromaClient({ baseUrl: 'http://kroma.test', fetch, authToken: 'old' });
    const refresh = vi.fn(async () => undefined);
    client.setRefreshHandler(refresh);
    await expect(client.home()).rejects.toMatchObject({ status: 401 });
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(calls).toHaveLength(1); // no retry
  });

  it('does not loop: a second 401 after the retry throws', async () => {
    // Always 401 on /home.
    const { fetch, calls } = recordingFetch(() => ({ ok: false, status: 401, json: {} }));
    const client = new KromaClient({ baseUrl: 'http://kroma.test', fetch, authToken: 'old' });
    const refresh = vi.fn(async () => 'fresh');
    client.setRefreshHandler(refresh);
    await expect(client.home()).rejects.toMatchObject({ status: 401 });
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(calls).toHaveLength(2); // original + one retry, then gives up
  });

  it('never refreshes a NO_REFRESH endpoint (token exchange)', async () => {
    const { fetch, calls } = recordingFetch(() => ({ ok: false, status: 401, json: {} }));
    const client = new KromaClient({ baseUrl: 'http://kroma.test', fetch, authToken: 'old' });
    const refresh = vi.fn(async () => 'fresh');
    client.setRefreshHandler(refresh);
    await expect(client.exchangeToken('access')).rejects.toMatchObject({ status: 401 });
    expect(refresh).not.toHaveBeenCalled();
    expect(calls).toHaveLength(1);
  });

  it('matches NO_REFRESH against the path without its query string', async () => {
    // quickconnect/poll is NO_REFRESH and carries a `?secret=` query.
    const { fetch, calls } = recordingFetch(() => ({ ok: false, status: 401, json: {} }));
    const client = new KromaClient({ baseUrl: 'http://kroma.test', fetch, authToken: 'old' });
    const refresh = vi.fn(async () => 'fresh');
    client.setRefreshHandler(refresh);
    await expect(client.quickConnectPoll('sec')).rejects.toMatchObject({ status: 401 });
    expect(refresh).not.toHaveBeenCalled();
    expect(calls).toHaveLength(1);
  });

  it('does refresh a refresh-eligible path that carries a query string', async () => {
    const { fetch, calls } = refreshingFetch('/search');
    const client = new KromaClient({ baseUrl: 'http://kroma.test', fetch, authToken: 'old' });
    const refresh = vi.fn(async () => 'fresh');
    client.setRefreshHandler(refresh);
    await client.search('star wars');
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(calls).toHaveLength(2);
  });

  it('does not refresh on a non-401 error', async () => {
    const { fetch, calls } = recordingFetch(() => ({ ok: false, status: 500, json: {} }));
    const client = new KromaClient({ baseUrl: 'http://kroma.test', fetch, authToken: 'old' });
    const refresh = vi.fn(async () => 'fresh');
    client.setRefreshHandler(refresh);
    await expect(client.home()).rejects.toMatchObject({ status: 500 });
    expect(refresh).not.toHaveBeenCalled();
    expect(calls).toHaveLength(1);
  });
});

describe('posterBlob', () => {
  it('fetches an absolute (TMDB) poster directly, no /api prefix or auth', async () => {
    const { client, calls } = makeClient(undefined, { authToken: 'tok' });
    const blob = await client.posterBlob({
      id: ItemId.of('i1'),
      metadata: { posterUrl: 'https://image.tmdb.org/p.jpg' } as any,
    });
    expect(blob).toBeInstanceOf(Blob);
    expect(calls[0]?.url).toBe('https://image.tmdb.org/p.jpg');
    // Direct fetch: no bearer attached (the request had no init headers).
    expect(calls[0]?.headers.get('Authorization')).toBeNull();
  });

  it('throws when an absolute poster fetch is not ok', async () => {
    const { client } = makeClient(() => ({ ok: false, status: 404 }), { authToken: 'tok' });
    await expect(
      client.posterBlob({
        id: ItemId.of('i1'),
        metadata: { posterUrl: 'https://img/x.jpg' } as any,
      }),
    ).rejects.toThrow('poster 404');
  });

  it('strips a single /api prefix from a cached-art path and refetches it', async () => {
    const { client, calls } = makeClient();
    await client.posterBlob({
      id: ItemId.of('i1'),
      metadata: { posterUrl: '/api/images/p.webp' } as any,
    });
    expect(calls[0]?.url).toBe('http://kroma.test/api/images/p.webp');
  });

  it('falls back to the generated poster endpoint when no posterUrl (encoding the id)', async () => {
    const { client, calls } = makeClient();
    await client.posterBlob({ id: ItemId.of('a b'), metadata: null });
    expect(calls[0]?.url).toBe('http://kroma.test/api/items/a%20b/poster');
  });
});

describe('URL builders (pure, no request)', () => {
  const c = new KromaClient({ baseUrl: 'http://kroma.test' });

  it('build stream / hls / poster / subtitle / storyboard / logs URLs', () => {
    expect(c.streamUrl('a b')).toBe('http://kroma.test/api/items/a%20b/stream');
    expect(c.hlsMasterUrl('id')).toBe('http://kroma.test/api/items/id/hls/copy/0/0/index.m3u8');
    expect(c.hlsMasterUrl('id', true, 600.6, 2)).toBe(
      'http://kroma.test/api/items/id/hls/aac/601/2/index.m3u8',
    );
    expect(c.posterUrl('id')).toBe('http://kroma.test/api/items/id/poster');
    expect(c.showPosterUrl('s1')).toBe('http://kroma.test/api/shows/s1/poster');
    expect(c.subtitleUrl('id', 3)).toBe('http://kroma.test/api/items/id/subtitles/3.vtt');
    expect(c.storyboardUrl('id')).toBe('http://kroma.test/api/items/id/storyboard');
    expect(c.logsUrl()).toBe('http://kroma.test/api/logs?tail=200');
    expect(c.logsUrl(50)).toBe('http://kroma.test/api/logs?tail=50');
  });

  it('resolve art / poster / backdrop / theme helpers', () => {
    expect(c.resolveArt('/api/images/x.webp')).toBe('http://kroma.test/api/images/x.webp');
    expect(c.resolveArt('https://cdn/x.jpg')).toBe('https://cdn/x.jpg');
    expect(c.resolveArt(null)).toBeNull();
    expect(c.posterFor({ id: 'i1', metadata: { posterUrl: '/api/p.webp' } as any })).toBe(
      'http://kroma.test/api/p.webp',
    );
    expect(c.posterFor({ id: 'i2', metadata: null })).toBe('http://kroma.test/api/items/i2/poster');
    expect(c.showPosterFor({ id: 's1', metadata: null } as any)).toBe(
      'http://kroma.test/api/shows/s1/poster',
    );
    expect(c.backdropFor({ metadata: { backdropUrl: '/api/b.webp' } as any })).toBe(
      'http://kroma.test/api/b.webp',
    );
    expect(c.backdropFor({ metadata: null })).toBeNull();
    expect(c.themeFor({ metadata: { themeUrl: '/api/t.mp3' } as any })).toBe(
      'http://kroma.test/api/t.mp3',
    );
    expect(c.themeFor({ metadata: null })).toBeNull();
  });
});

describe('delegating methods issue the expected request', () => {
  // Exact path + method assertions for the domains whose paths this test owns.
  const known: Array<[string, (c: KromaClient) => unknown, string, string]> = [
    ['health', (c) => c.health(), 'GET', '/health'],
    ['modules', (c) => c.modules(), 'GET', '/modules'],
    ['libraries', (c) => c.libraries(), 'GET', '/libraries'],
    ['items', (c) => c.items(), 'GET', '/items'],
    ['items(lib)', (c) => c.items('lib1'), 'GET', '/items?library=lib1'],
    ['movies', (c) => c.movies(), 'GET', '/movies'],
    ['shows', (c) => c.shows(), 'GET', '/shows'],
    ['show', (c) => c.show('s1'), 'GET', '/shows/s1'],
    ['item', (c) => c.item('i1'), 'GET', '/items/i1'],
    ['similar', (c) => c.similar('i1'), 'GET', '/items/i1/similar'],
    ['themed', (c) => c.themed('q x'), 'GET', '/themed?q=q%20x'],
    ['home', (c) => c.home(), 'GET', '/home'],
    ['aiSuggest', (c) => c.aiSuggest('i1'), 'GET', '/items/i1/ai-suggest'],
    ['search', (c) => c.search('q'), 'GET', '/search?q=q'],
    ['personCredits', (c) => c.personCredits('n'), 'GET', '/people?name=n'],
    ['scan', (c) => c.scan(), 'POST', '/scan'],
    ['status', (c) => c.status(), 'GET', '/status'],
    ['register', (c) => c.register('e', 'u', 'p'), 'POST', '/auth/register'],
    ['login', (c) => c.login('id', 'p'), 'POST', '/auth/login'],
    ['exchangeToken', (c) => c.exchangeToken('at'), 'POST', '/auth/token'],
    ['relock', (c) => c.relock('at'), 'POST', '/auth/relock'],
    ['logout', (c) => c.logout(), 'POST', '/auth/logout'],
    ['me', (c) => c.me(), 'GET', '/auth/me'],
    ['updateAccount', (c) => c.updateAccount({ username: 'x' }), 'PATCH', '/auth/me'],
    ['authConfig', (c) => c.authConfig(), 'GET', '/auth/config'],
    ['users', (c) => c.users(), 'GET', '/users'],
    ['listSessions', (c) => c.listSessions(), 'GET', '/auth/me/sessions'],
    ['revokeSession', (c) => c.revokeSession('s1'), 'DELETE', '/auth/me/sessions/s1'],
    ['invites', (c) => c.invites(), 'GET', '/invites'],
    ['checkInvite', (c) => c.checkInvite('t'), 'GET', '/invites/t'],
    ['revokeInvite', (c) => c.revokeInvite('t'), 'DELETE', '/invites/t'],
    ['quickConnectPoll', (c) => c.quickConnectPoll('s'), 'GET', '/auth/quickconnect/poll?secret=s'],
    ['adminLibraries', (c) => c.adminLibraries(), 'GET', '/admin/libraries'],
    [
      'createLibrary',
      (c) => c.createLibrary({ name: 'n', folders: [] }),
      'POST',
      '/admin/libraries',
    ],
    ['updateLibrary', (c) => c.updateLibrary('x', {}), 'PATCH', '/admin/libraries/x'],
    ['deleteLibrary', (c) => c.deleteLibrary('x'), 'DELETE', '/admin/libraries/x'],
    ['scanLibrary', (c) => c.scanLibrary('x'), 'POST', '/admin/libraries/x/scan'],
    ['adminNaming', (c) => c.adminNaming(), 'GET', '/admin/organize/naming'],
    ['namingSample', (c) => c.namingSample({} as any), 'POST', '/admin/organize/sample'],
    ['saveNaming', (c) => c.saveNaming({} as any), 'PUT', '/admin/organize/naming'],
    ['organizePreview', (c) => c.organizePreview(), 'GET', '/admin/organize/preview'],
    ['organizeApply', (c) => c.organizeApply(), 'POST', '/admin/organize/apply'],
  ];

  it.each(known)('%s -> %s %s', async (_name, call, method, path) => {
    const { client, calls } = makeClient();
    await Promise.resolve(call(client)).catch(() => undefined);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.method).toBe(method);
    expect(calls[0]?.url).toBe(`http://kroma.test/api${path}`);
  });

  // The remaining delegates: assert only that exactly one request is issued (the
  // per-domain paths/methods are covered by their own client/*.test.ts). This
  // exercises every facade line without duplicating those assertions.
  const others: Array<[string, (c: KromaClient) => unknown]> = [
    ['downloadedSubtitles', (c) => c.downloadedSubtitles('i1')],
    ['subtitleCapabilities', (c) => c.subtitleCapabilities('i1')],
    ['generateSubtitle', (c) => c.generateSubtitle('i1', {} as any)],
    ['subtitleGenerations', (c) => c.subtitleGenerations('i1')],
    ['cancelGeneration', (c) => c.cancelGeneration('i1', 'g1')],
    ['deleteSubtitle', (c) => c.deleteSubtitle('i1', 'd1')],
    ['createInvite', (c) => c.createInvite()],
    ['updateLanguage', (c) => c.updateLanguage('fr')],
    ['changePassword', (c) => c.changePassword('a', 'bbbb')],
    ['passkeyRegisterStart', (c) => c.passkeyRegisterStart()],
    [
      'passkeyRegisterFinish',
      (c) => c.passkeyRegisterFinish({ ceremonyId: 'c', name: 'n', credential: {} as any }),
    ],
    ['listPasskeys', (c) => c.listPasskeys()],
    ['deletePasskey', (c) => c.deletePasskey('p1')],
    ['passkeyAuthStart', (c) => c.passkeyAuthStart()],
    ['passkeyAuthFinish', (c) => c.passkeyAuthFinish({ ceremonyId: 'c', credential: {} as any })],
    ['pinVerify', (c) => c.pinVerify('1234')],
    ['setPin', (c) => c.setPin('1234')],
    ['clearPin', (c) => c.clearPin('1234')],
    ['uploadAvatar', (c) => c.uploadAvatar(new Blob(['x']))],
    ['quickConnectInitiate', (c) => c.quickConnectInitiate()],
    ['quickConnectAuthorize', (c) => c.quickConnectAuthorize('code')],
    ['progress', (c) => c.progress()],
    ['itemProgress', (c) => c.itemProgress('i1')],
    ['continueWatching', (c) => c.continueWatching()],
    ['upNext', (c) => c.upNext('s1')],
    ['nextEpisode', (c) => c.nextEpisode('i1')],
    ['followingEpisodes', (c) => c.followingEpisodes('i1')],
    ['forYou', (c) => c.forYou()],
    ['saveProgress', (c) => c.saveProgress('i1', 1000)],
    ['deleteProgress', (c) => c.deleteProgress('i1')],
    ['watched', (c) => c.watched()],
    ['markWatched', (c) => c.markWatched('i1')],
    ['unmarkWatched', (c) => c.unmarkWatched('i1')],
    ['myList', (c) => c.myList()],
    ['addToList', (c) => c.addToList('i1')],
    ['removeFromList', (c) => c.removeFromList('i1')],
    ['pingPlayback', (c) => c.pingPlayback({ sessionId: 's', itemId: 'i', positionMs: 0 })],
    ['stopPlayback', (c) => c.stopPlayback('sess')],
    ['discoverSearch', (c) => c.discoverSearch('q')],
    ['discoverTrending', (c) => c.discoverTrending()],
    ['discoverDetail', (c) => c.discoverDetail('movie', 1)],
    ['listRequests', (c) => c.listRequests()],
    ['getCalendar', (c) => c.getCalendar()],
    ['getMissing', (c) => c.getMissing()],
    ['searchAllMissing', (c) => c.searchAllMissing()],
    ['autoSearchRequest', (c) => c.autoSearchRequest('r1')],
    ['createRequest', (c) => c.createRequest({} as any)],
    ['deleteRequest', (c) => c.deleteRequest('r1')],
    ['approveRequest', (c) => c.approveRequest('r1')],
    ['denyRequest', (c) => c.denyRequest('r1')],
    ['searchReleases', (c) => c.searchReleases('r1')],
    ['grabRelease', (c) => c.grabRelease('r1', {} as any)],
    ['adminIndexers', (c) => c.adminIndexers()],
    ['createIndexer', (c) => c.createIndexer({} as any)],
    ['updateIndexer', (c) => c.updateIndexer('x', {} as any)],
    ['deleteIndexer', (c) => c.deleteIndexer('x')],
    ['testIndexer', (c) => c.testIndexer('x')],
    ['adminIndexerDefinitions', (c) => c.adminIndexerDefinitions()],
    ['indexerDefinitionDetail', (c) => c.indexerDefinitionDetail('x')],
    ['syncIndexerDefinitions', (c) => c.syncIndexerDefinitions()],
    ['adminDownloadClients', (c) => c.adminDownloadClients()],
    ['createDownloadClient', (c) => c.createDownloadClient({} as any)],
    ['updateDownloadClient', (c) => c.updateDownloadClient('x', {} as any)],
    ['deleteDownloadClient', (c) => c.deleteDownloadClient('x')],
    ['testDownloadClient', (c) => c.testDownloadClient('x')],
    ['adminDownloads', (c) => c.adminDownloads()],
    ['pauseDownload', (c) => c.pauseDownload('x')],
    ['resumeDownload', (c) => c.resumeDownload('x')],
    ['retryDownload', (c) => c.retryDownload('x')],
    ['reannounceDownload', (c) => c.reannounceDownload('x')],
    ['pauseAllDownloads', (c) => c.pauseAllDownloads()],
    ['resumeAllDownloads', (c) => c.resumeAllDownloads()],
    ['reannounceDownloads', (c) => c.reannounceDownloads()],
    ['removeDownload', (c) => c.removeDownload('x')],
    ['manualSearch', (c) => c.manualSearch('q')],
    ['analyzeTorrent', (c) => c.analyzeTorrent('magnet:?x')],
    ['manualAdd', (c) => c.manualAdd({} as any)],
    ['adminVpn', (c) => c.adminVpn()],
    ['saveVpn', (c) => c.saveVpn({} as any)],
    ['testVpn', (c) => c.testVpn()],
    ['adminBrowseFolders', (c) => c.adminBrowseFolders()],
    ['adminServer', (c) => c.adminServer()],
    ['adminSessions', (c) => c.adminSessions()],
    ['terminateSession', (c) => c.terminateSession('x')],
    ['adminMetrics', (c) => c.adminMetrics()],
    ['adminStorage', (c) => c.adminStorage()],
    ['clearCache', (c) => c.clearCache()],
    ['resetMetadata', (c) => c.resetMetadata()],
    ['adminUsers', (c) => c.adminUsers()],
    ['updateUser', (c) => c.updateUser('x', {})],
    ['deleteUser', (c) => c.deleteUser('x')],
    ['adminSettings', (c) => c.adminSettings('view')],
    ['updateSettings', (c) => c.updateSettings({})],
    ['exportBackup', (c) => c.exportBackup()],
    ['importBackup', (c) => c.importBackup(new Blob(['x']))],
    ['topUsers', (c) => c.topUsers()],
    ['playHistory', (c) => c.playHistory()],
    ['adminOverview', (c) => c.adminOverview()],
    ['adminLogs', (c) => c.adminLogs()],
    ['adminJobs', (c) => c.adminJobs()],
    ['adminJob', (c) => c.adminJob('k')],
    ['runJob', (c) => c.runJob('k')],
    ['cancelJob', (c) => c.cancelJob('k')],
    ['updateJob', (c) => c.updateJob('k', {})],
    ['jobRunLogs', (c) => c.jobRunLogs('r1')],
    ['adminPipeline', (c) => c.adminPipeline()],
    ['pipelineFailed', (c) => c.pipelineFailed('s')],
    ['runPipelineStage', (c) => c.runPipelineStage('s')],
    ['cancelPipelineStage', (c) => c.cancelPipelineStage('s')],
    ['pausePipeline', (c) => c.pausePipeline(true)],
    ['retryPipelineStage', (c) => c.retryPipelineStage('s')],
    ['reprocessPipelineStage', (c) => c.reprocessPipelineStage('s')],
    ['retryPipelineTask', (c) => c.retryPipelineTask('s', 'sub')],
    ['reprocessSubject', (c) => c.reprocessSubject('item', 'i1')],
    ['itemProcessing', (c) => c.itemProcessing('i1')],
    ['pipelineElements', (c) => c.pipelineElements()],
    ['retryElementStage', (c) => c.retryElementStage('item', 'i1', 's')],
    ['showProcessing', (c) => c.showProcessing('s1')],
    ['adminLlm', (c) => c.adminLlm()],
    ['saveLlm', (c) => c.saveLlm({} as any)],
    ['llmModels', (c) => c.llmModels({} as any)],
    ['testLlm', (c) => c.testLlm({} as any)],
    ['adminRemote', (c) => c.adminRemote()],
    ['saveRemote', (c) => c.saveRemote({} as any)],
    ['logs', (c) => c.logs()],
    ['storyboard', (c) => c.storyboard('i1')],
  ];

  it.each(others)('%s issues exactly one request', async (_name, call) => {
    const { client, calls } = makeClient();
    await Promise.resolve(call(client)).catch(() => undefined);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url.startsWith('http://kroma.test/')).toBe(true);
  });
});
