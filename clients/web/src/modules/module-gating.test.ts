import { describe, expect, it } from 'vitest';
import { hiddenModuleIds } from './module-gating';

const REGISTERED = ['tv.kroma.vpn', 'tv.kroma.torrents', 'tv.kroma.remote', 'tv.kroma.indexer'];

describe('hiddenModuleIds', () => {
  it('hides a bundled UI whose backend module is not installed at all', () => {
    // Only indexer is installed server-side: vpn/torrents/remote must hide
    // (the ghost-sidebar bug: uninstalled modules stayed visible).
    const hidden = hiddenModuleIds([{ id: 'tv.kroma.indexer', enabled: true }], REGISTERED);
    expect(hidden.has('tv.kroma.vpn')).toBe(true);
    expect(hidden.has('tv.kroma.torrents')).toBe(true);
    expect(hidden.has('tv.kroma.remote')).toBe(true);
    expect(hidden.has('tv.kroma.indexer')).toBe(false);
  });

  it('hides an installed-but-disabled module', () => {
    const hidden = hiddenModuleIds([{ id: 'tv.kroma.indexer', enabled: false }], REGISTERED);
    expect(hidden.has('tv.kroma.indexer')).toBe(true);
  });

  it('shows installed modules with enabled true or omitted', () => {
    const hidden = hiddenModuleIds(
      [{ id: 'tv.kroma.vpn', enabled: true }, { id: 'tv.kroma.torrents' }],
      REGISTERED,
    );
    expect(hidden.has('tv.kroma.vpn')).toBe(false);
    expect(hidden.has('tv.kroma.torrents')).toBe(false);
  });

  it('hides everything registered when the server lists no modules', () => {
    expect(hiddenModuleIds([], REGISTERED).size).toBe(REGISTERED.length);
  });

  it('hides nothing while the backend list has not resolved yet', () => {
    expect(hiddenModuleIds(undefined, REGISTERED).size).toBe(0);
  });
});
