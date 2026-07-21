import { describe, expect, it } from 'vitest';
import {
  archSupported,
  cmpDsmVersion,
  DEFAULT_REPO,
  dsmVersion,
  type Entry,
  entryVersion,
  type SpkInfo,
  toDsmPackage,
  versionFromSpkName,
} from './catalog';

function entry(over: Partial<Entry> = {}): Entry {
  return {
    channel: 'stable',
    tag: 'v0.1.25',
    releaseName: 'KROMA 0.1.25',
    releaseUrl: 'https://github.com/maxscharwath/kroma/releases/tag/v0.1.25',
    publishedAt: '2026-07-01T00:00:00Z',
    spkName: 'kroma-0.1.25-3439372-x86_64.spk',
    spkUrl: 'https://example/kroma.spk',
    spkSize: 1048576,
    info: null,
    ...over,
  };
}

const info: SpkInfo = {
  package: 'kroma',
  version: '0.1.25-3439372',
  dname: 'KROMA',
  desc: 'desc',
  arch: 'x86_64',
  firmware: '7.0-40000',
  size: 2097152,
  md5: 'abc123',
  beta: false,
};

describe('versionFromSpkName', () => {
  it('extracts version-build from a kroma spk name', () => {
    expect(versionFromSpkName('kroma-0.1.25-3439372-x86_64.spk')).toBe('0.1.25-3439372');
  });

  it('parses a pre-rebrand luma spk name (prefix-agnostic)', () => {
    expect(versionFromSpkName('luma-1.0.0-deadbee-x64.spk')).toBe('1.0.0-deadbee');
  });

  it('falls back to the name minus .spk when the pattern does not match', () => {
    expect(versionFromSpkName('weird.spk')).toBe('weird');
    // Uppercase prefix does not match [a-z]+, so it falls back too.
    expect(versionFromSpkName('KROMA-1.0-x64.spk')).toBe('KROMA-1.0-x64');
  });
});

describe('entryVersion', () => {
  it('prefers the sidecar info version', () => {
    expect(entryVersion(entry({ info }))).toBe('0.1.25-3439372');
  });

  it('derives from the spk name when there is no sidecar', () => {
    expect(entryVersion(entry())).toBe('0.1.25-3439372');
  });
});

describe('cmpDsmVersion', () => {
  it('orders by dotted feature segments numerically', () => {
    expect(cmpDsmVersion('1.2.0', '1.10.0')).toBeLessThan(0);
    expect(cmpDsmVersion('2.0.0', '1.9.9')).toBeGreaterThan(0);
  });

  it('compares the build suffix when the feature version ties', () => {
    expect(cmpDsmVersion('0.1.25-3439372', '0.1.25-3439300')).toBeGreaterThan(0);
    expect(cmpDsmVersion('0.1.25-100', '0.1.25-100')).toBe(0);
  });

  it('treats a missing build as 0 and missing segments as 0', () => {
    expect(cmpDsmVersion('1.0', '1.0.0')).toBe(0);
    expect(cmpDsmVersion('1.0.1', '1.0')).toBeGreaterThan(0);
  });

  it('tolerates non-numeric segments as 0', () => {
    expect(cmpDsmVersion('x.y', 'a.b')).toBe(0);
  });
});

describe('archSupported', () => {
  it('is permissive for a null arch', () => {
    expect(archSupported(null)).toBe(true);
  });

  it('accepts the x86_64 codename families (case-insensitive) and noarch', () => {
    expect(archSupported('x86_64')).toBe(true);
    expect(archSupported('X64')).toBe(true);
    expect(archSupported('apollolake')).toBe(true);
    expect(archSupported('EPYC7002')).toBe(true);
    expect(archSupported('noarch')).toBe(true);
  });

  it('rejects an unrelated arch', () => {
    expect(archSupported('armv7')).toBe(false);
    expect(archSupported('aarch64')).toBe(false);
  });
});

describe('dsmVersion', () => {
  it('leaves a conventional 3-segment version untouched', () => {
    expect(dsmVersion('0.1.31-3447024')).toBe('0.1.31-3447024');
  });

  it('collapses a nightly X.Y.Z.BUILD-BUILD to X.Y.Z-BUILD (DSM hides the 4th segment)', () => {
    expect(dsmVersion('0.1.31.3447024-3447024')).toBe('0.1.31-3447024');
  });

  it('handles a build-less and a short version', () => {
    expect(dsmVersion('1.2.3.4')).toBe('1.2.3');
    expect(dsmVersion('23.10-3')).toBe('23.10-3');
  });
});

describe('toDsmPackage', () => {
  it('fills defaults when there is no sidecar info', () => {
    const pkg = toDsmPackage(entry(), 'https://pkg.kroma.tv', 'maxscharwath/kroma');
    expect(pkg.package).toBe('kroma');
    expect(pkg.version).toBe('0.1.25-3439372');
    expect(pkg.dname).toBe('KROMA');
    expect(pkg.link).toBe('https://example/kroma.spk');
    expect(pkg.size).toBe(1048576); // falls back to entry spkSize
    expect(pkg.md5).toBeUndefined();
    expect(pkg.firmware).toBe('7.0-40000');
    // No `beta` field at all - DSM hides `beta:true` from a dynamic source.
    expect('beta' in pkg).toBe(false);
    expect(pkg.thumbnail).toEqual(['https://pkg.kroma.tv/icon.png']);
    expect(pkg.maintainer_url).toBe('https://github.com/maxscharwath/kroma');
    expect(pkg.changelog).toBe(entry().releaseUrl);
  });

  it('prefers sidecar fields and never emits a beta flag for a nightly', () => {
    const pkg = toDsmPackage(
      entry({ info, channel: 'nightly' }),
      'https://pkg.kroma.tv',
      'maxscharwath/kroma',
    );
    expect(pkg.version).toBe('0.1.25-3439372');
    expect(pkg.size).toBe(2097152); // sidecar size wins
    expect(pkg.md5).toBe('abc123');
    expect(pkg.desc).toBe('desc');
    expect('beta' in pkg).toBe(false);
  });
});

describe('DEFAULT_REPO', () => {
  it('points at the kroma repo', () => {
    expect(DEFAULT_REPO).toBe('maxscharwath/kroma');
  });
});
