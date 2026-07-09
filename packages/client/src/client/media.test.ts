import { describe, expect, it } from 'vitest';
import type { RequestContext } from './base';
import { hlsMasterUrl } from './media';

// `hlsMasterUrl` only reads `ctx.baseUrl`, so a minimal stub suffices.
const ctx = { baseUrl: 'http://luma.test' } as unknown as RequestContext;

describe('hlsMasterUrl', () => {
  it('emits the copy program at anchor 0, audio 0', () => {
    expect(hlsMasterUrl(ctx, 'abc')).toBe('http://luma.test/api/items/abc/hls/copy/0/0/index.m3u8');
  });

  it('emits the aac program for the AAC variant', () => {
    expect(hlsMasterUrl(ctx, 'abc', true)).toBe(
      'http://luma.test/api/items/abc/hls/aac/0/0/index.m3u8',
    );
  });

  it('puts the anchor (rounded, clamped) and audio track in the path', () => {
    expect(hlsMasterUrl(ctx, 'abc', false, 600.4, 1)).toBe(
      'http://luma.test/api/items/abc/hls/copy/600/1/index.m3u8',
    );
    expect(hlsMasterUrl(ctx, 'abc', false, -5, 0)).toBe(
      'http://luma.test/api/items/abc/hls/copy/0/0/index.m3u8',
    );
  });

  it('url-encodes the item id', () => {
    expect(hlsMasterUrl(ctx, 'a b/c', true, 0, 2)).toBe(
      'http://luma.test/api/items/a%20b%2Fc/hls/aac/0/2/index.m3u8',
    );
  });
});
