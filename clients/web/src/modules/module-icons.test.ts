import { IconApps, IconChartBar, IconDeviceTv, IconMovie } from '@tabler/icons-react';
import { describe, expect, it } from 'vitest';
import { resolveModuleIcon } from './module-icons';

describe('resolveModuleIcon', () => {
  it('maps known names to their icon component', () => {
    expect(resolveModuleIcon('movie')).toBe(IconMovie);
    expect(resolveModuleIcon('tv')).toBe(IconDeviceTv);
    expect(resolveModuleIcon('chart')).toBe(IconChartBar);
  });

  it('maps aliases to the same icon (stats -> chart)', () => {
    expect(resolveModuleIcon('stats')).toBe(IconChartBar);
    expect(resolveModuleIcon('stats')).toBe(resolveModuleIcon('chart'));
  });

  it('falls back to the apps icon for unknown / empty / missing names', () => {
    expect(resolveModuleIcon('does-not-exist')).toBe(IconApps);
    expect(resolveModuleIcon('')).toBe(IconApps);
    expect(resolveModuleIcon(undefined)).toBe(IconApps);
  });
});
