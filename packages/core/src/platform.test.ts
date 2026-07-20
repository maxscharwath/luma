import { afterEach, describe, expect, it, vi } from 'vitest';
import { isTizenRuntime, isWebOsRuntime } from './platform';

afterEach(() => {
  vi.unstubAllGlobals();
});

const DESKTOP_UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/126.0';

describe('isTizenRuntime', () => {
  it('matches the Samsung TV user agent', () => {
    expect(isTizenRuntime('Mozilla/5.0 (SMART-TV; LINUX; Tizen 6.0)')).toBe(true);
    expect(isTizenRuntime(DESKTOP_UA)).toBe(false);
  });

  it('matches the injected tizen bridge whatever the user agent says', () => {
    vi.stubGlobal('tizen', {});
    expect(isTizenRuntime(DESKTOP_UA)).toBe(true);
  });
});

describe('isWebOsRuntime', () => {
  it('matches both LG spellings (digit zero and letter O)', () => {
    expect(isWebOsRuntime('Mozilla/5.0 (Web0S; Linux/SmartTV)')).toBe(true);
    expect(isWebOsRuntime('Mozilla/5.0 (webOS; Linux/SmartTV)')).toBe(true);
    expect(isWebOsRuntime(DESKTOP_UA)).toBe(false);
  });

  it('does not match a stray "webs"', () => {
    expect(isWebOsRuntime('Mozilla/5.0 (cobwebs)')).toBe(false);
  });

  it('matches the injected webOS bridge whatever the user agent says', () => {
    vi.stubGlobal('webOS', { service: {} });
    expect(isWebOsRuntime(DESKTOP_UA)).toBe(true);
  });
});
