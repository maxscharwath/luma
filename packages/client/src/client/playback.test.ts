import { describe, expect, it } from 'vitest';
import type { PlaybackPing } from '../types';
import type { RequestContext } from './base';
import {
  addToList,
  deleteProgress,
  followingEpisodes,
  itemProgress,
  markWatched,
  nextEpisode,
  pingPlayback,
  saveProgress,
  stopPlayback,
  upNext,
} from './playback';

function recordCtx() {
  const calls: { path: string; init?: RequestInit }[] = [];
  const ctx = {
    baseUrl: 'http://nas',
    json: async (path: string, init?: RequestInit) => {
      calls.push({ path, init });
      return {} as never;
    },
  } as unknown as RequestContext;
  return { ctx, calls };
}

describe('saveProgress', () => {
  it('PUTs a rounded position and null-defaulted duration', () => {
    const { ctx, calls } = recordCtx();
    void saveProgress(ctx, 'a b', 1234.7);
    expect(calls[0]?.path).toBe('/progress/a%20b');
    expect(calls[0]?.init?.method).toBe('PUT');
    expect(JSON.parse(calls[0]?.init?.body as string)).toEqual({
      positionMs: 1235,
      durationMs: null,
    });
  });

  it('passes a supplied duration through', () => {
    const { ctx, calls } = recordCtx();
    void saveProgress(ctx, 'x', 10.2, 7200000);
    expect(JSON.parse(calls[0]?.init?.body as string)).toEqual({
      positionMs: 10,
      durationMs: 7200000,
    });
  });
});

describe('progress + resume URLs', () => {
  it('encodes ids in the resume/next/progress paths', () => {
    const { ctx, calls } = recordCtx();
    void upNext(ctx, 's 1');
    void nextEpisode(ctx, 'i 2');
    void followingEpisodes(ctx, 'i 5');
    void itemProgress(ctx, 'i 3');
    void deleteProgress(ctx, 'i 4');
    expect(calls.map((c) => c.path)).toEqual([
      '/shows/s%201/up-next',
      '/items/i%202/next',
      '/items/i%205/following',
      '/progress/i%203',
      '/progress/i%204',
    ]);
    expect(calls[4]?.init?.method).toBe('DELETE');
  });
});

describe('watched / list toggles', () => {
  it('PUTs to the encoded watched + my-list endpoints', () => {
    const { ctx, calls } = recordCtx();
    void markWatched(ctx, 'i 1');
    void addToList(ctx, 'i 2');
    expect(calls[0]).toMatchObject({ path: '/watched/i%201', init: { method: 'PUT' } });
    expect(calls[1]).toMatchObject({ path: '/my-list/i%202', init: { method: 'PUT' } });
  });
});

describe('heartbeats', () => {
  it('POSTs the ping payload and the stop sessionId', () => {
    const { ctx, calls } = recordCtx();
    const ping: PlaybackPing = { sessionId: 's', itemId: 'i', positionMs: 5, state: 'playing' };
    void pingPlayback(ctx, ping);
    void stopPlayback(ctx, 'sess-9');
    expect(calls[0]?.path).toBe('/playback/ping');
    expect(JSON.parse(calls[0]?.init?.body as string)).toEqual(ping);
    expect(calls[1]?.path).toBe('/playback/stop');
    expect(JSON.parse(calls[1]?.init?.body as string)).toEqual({ sessionId: 'sess-9' });
  });
});
