import { describe, expect, it } from 'vitest';
import { artUrl, hostOf } from './util';

describe('hostOf', () => {
  it('returns the hostname of a valid URL', () => {
    expect(hostOf('http://nas.local:4040/app')).toBe('nas.local');
    expect(hostOf('https://kroma.tv')).toBe('kroma.tv');
  });

  it('returns null for an unparseable URL', () => {
    expect(hostOf('not a url')).toBeNull();
    expect(hostOf('')).toBeNull();
  });
});

describe('artUrl', () => {
  it('returns null for a missing url', () => {
    expect(artUrl('http://nas:4040', null)).toBeNull();
    expect(artUrl('http://nas:4040', undefined)).toBeNull();
    expect(artUrl('http://nas:4040', '')).toBeNull();
  });

  it('passes absolute urls through untouched', () => {
    expect(artUrl('http://nas:4040', 'https://cdn/x.jpg')).toBe('https://cdn/x.jpg');
    expect(artUrl('http://nas:4040', 'http://other/y.png')).toBe('http://other/y.png');
  });

  it('resolves a relative path against the normalized server url', () => {
    // Trailing slashes on the server url are normalized away first.
    expect(artUrl('http://nas:4040/', '/avatars/a.webp')).toBe('http://nas:4040/avatars/a.webp');
    // A path without a leading slash gets one.
    expect(artUrl('http://nas:4040', 'avatars/a.webp')).toBe('http://nas:4040/avatars/a.webp');
  });
});
