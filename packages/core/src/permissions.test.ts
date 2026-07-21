import { describe, expect, it } from 'vitest';
import { PERMISSIONS } from './permissions';

describe('PERMISSIONS', () => {
  it('lists every grantable permission with unique keys', () => {
    const keys = PERMISSIONS.map((p) => p.key);
    expect(keys).toEqual([
      'playback',
      'library.manage',
      'users.manage',
      'settings.manage',
      'requests.create',
      'requests.manage',
      'requests.auto',
      'reports.manage',
    ]);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it('gives every entry a label and hint i18n key', () => {
    for (const p of PERMISSIONS) {
      expect(p.labelKey).toMatch(/^admin\.perm/);
      expect(p.hintKey).toMatch(/^admin\.perm.*Hint$/);
    }
  });
});
