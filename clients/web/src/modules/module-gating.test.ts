import { describe, expect, it } from 'vitest';
import { hiddenModuleIds } from './module-gating';

const REGISTERED = ['dev.luma.vpn', 'dev.luma.torrents', 'dev.luma.remote', 'dev.luma.indexer'];

describe('hiddenModuleIds', () => {
  it('hides a bundled UI whose backend module is not installed at all', () => {
    // Only indexer is installed server-side: vpn/torrents/remote must hide
    // (the ghost-sidebar bug: uninstalled modules stayed visible).
    const hidden = hiddenModuleIds([{ id: 'dev.luma.indexer', enabled: true }], REGISTERED);
    expect(hidden.has('dev.luma.vpn')).toBe(true);
    expect(hidden.has('dev.luma.torrents')).toBe(true);
    expect(hidden.has('dev.luma.remote')).toBe(true);
    expect(hidden.has('dev.luma.indexer')).toBe(false);
  });

  it('hides an installed-but-disabled module', () => {
    const hidden = hiddenModuleIds([{ id: 'dev.luma.indexer', enabled: false }], REGISTERED);
    expect(hidden.has('dev.luma.indexer')).toBe(true);
  });

  it('shows installed modules with enabled true or omitted', () => {
    const hidden = hiddenModuleIds(
      [{ id: 'dev.luma.vpn', enabled: true }, { id: 'dev.luma.torrents' }],
      REGISTERED,
    );
    expect(hidden.has('dev.luma.vpn')).toBe(false);
    expect(hidden.has('dev.luma.torrents')).toBe(false);
  });

  it('hides everything registered when the server lists no modules', () => {
    expect(hiddenModuleIds([], REGISTERED).size).toBe(REGISTERED.length);
  });

  it('hides nothing while the backend list has not resolved yet', () => {
    expect(hiddenModuleIds(undefined, REGISTERED).size).toBe(0);
  });
});
