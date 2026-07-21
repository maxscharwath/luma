import { describe, expect, it } from 'vitest';
import { type ClientBuild, checkServerCompat, compareVersions } from './compat';

describe('compareVersions', () => {
  it('orders dotted numeric versions', () => {
    expect(compareVersions('0.1.31', '0.1.30')).toBe(1);
    expect(compareVersions('0.1.30', '0.1.31')).toBe(-1);
    expect(compareVersions('0.1.31', '0.1.31')).toBe(0);
    expect(compareVersions('0.2.0', '0.1.99')).toBe(1);
    expect(compareVersions('1.0.0', '0.9.9')).toBe(1);
  });

  it('ignores trailing suffixes and pads missing parts', () => {
    expect(compareVersions('0.1.31-rc1', '0.1.31')).toBe(0);
    expect(compareVersions('0.2', '0.2.0')).toBe(0);
    expect(compareVersions('0.2.1', '0.2')).toBe(1);
  });
});

describe('checkServerCompat', () => {
  const client = (over: Partial<ClientBuild> = {}): ClientBuild => ({
    version: '0.1.31',
    minServerVersion: '0.1.0',
    ...over,
  });

  it('is ok when the server meets the client requirement', () => {
    expect(checkServerCompat(client(), '0.1.31')).toBe('ok');
    expect(checkServerCompat(client({ minServerVersion: '0.1.30' }), '0.1.31')).toBe('ok');
  });

  it('flags the server as outdated when it predates the client requirement', () => {
    expect(checkServerCompat(client({ minServerVersion: '0.2.0' }), '0.1.31')).toBe(
      'server-outdated',
    );
  });

  it('never warns on dev/unknown placeholders', () => {
    expect(checkServerCompat(client({ minServerVersion: 'dev' }), '0.1.31')).toBe('ok');
    expect(checkServerCompat(client(), 'unknown')).toBe('ok');
  });
});
