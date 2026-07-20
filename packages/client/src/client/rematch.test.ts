import { describe, expect, it } from 'vitest';
import type { RequestContext } from './base';
import { matchCandidates, setMatch } from './rematch';

const EMPTY = { query: 'Dune', year: 2021, currentTmdbId: null, pinned: false, results: [] };

function recordCtx(reply: unknown = EMPTY) {
  const calls: { path: string; init?: RequestInit }[] = [];
  const ctx = {
    baseUrl: 'http://nas:4040',
    json: async (path: string, init?: RequestInit) => {
      calls.push({ path, init });
      return reply as never;
    },
  } as unknown as RequestContext;
  return { ctx, calls };
}

describe('matchCandidates', () => {
  it('searches the parsed title when no query is given', async () => {
    const { ctx, calls } = recordCtx();
    await matchCandidates(ctx, 'movie', 'item-1');
    expect(calls[0]?.path).toBe('/rematch/movie/item-1/candidates');
  });

  it('encodes a typed query', async () => {
    const { ctx, calls } = recordCtx();
    await matchCandidates(ctx, 'show', 'show-9', 'the wire & co');
    expect(calls[0]?.path).toBe('/rematch/show/show-9/candidates?q=the%20wire%20%26%20co');
  });

  it('treats a whitespace-only query as absent', async () => {
    const { ctx, calls } = recordCtx();
    await matchCandidates(ctx, 'movie', 'item-1', '   ');
    expect(calls[0]?.path).toBe('/rematch/movie/item-1/candidates');
  });

  it('parses the response through the schema', async () => {
    const { ctx } = recordCtx({
      ...EMPTY,
      results: [{ tmdbId: 603, title: 'The Matrix', score: 0.98, current: true }],
    });
    const out = await matchCandidates(ctx, 'movie', 'item-1');
    expect(out.results[0]?.tmdbId).toBe(603);
    expect(out.results[0]?.current).toBe(true);
  });

  it('rejects a malformed response rather than passing it on', async () => {
    const { ctx } = recordCtx({ ...EMPTY, results: [{ tmdbId: 'not-a-number' }] });
    await expect(matchCandidates(ctx, 'movie', 'item-1')).rejects.toThrow();
  });
});

describe('setMatch', () => {
  it('posts the chosen id', async () => {
    const { ctx, calls } = recordCtx();
    await setMatch(ctx, 'movie', 'item-1', 603);
    expect(calls[0]?.path).toBe('/rematch/movie/item-1');
    expect(calls[0]?.init?.method).toBe('POST');
    expect(JSON.parse(calls[0]?.init?.body as string)).toEqual({ tmdbId: 603 });
  });

  it('posts an explicit null to restore automatic matching', async () => {
    const { ctx, calls } = recordCtx();
    await setMatch(ctx, 'show', 'show-9', null);
    expect(JSON.parse(calls[0]?.init?.body as string)).toEqual({ tmdbId: null });
  });
});
