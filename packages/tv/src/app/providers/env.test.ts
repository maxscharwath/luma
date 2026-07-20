import { afterEach, describe, expect, it, vi } from 'vitest';
import { computeEnv } from './env';

function stubUa(userAgent: string) {
  vi.stubGlobal('navigator', { userAgent });
}

const DESKTOP_UA = 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/126.0';

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('computeEnv physicalKeyboard', () => {
  it('is off for a Tizen build running on a real Tizen TV', () => {
    stubUa('Mozilla/5.0 (SMART-TV; LINUX; Tizen 6.0)');
    expect(computeEnv('Tizen').physicalKeyboard).toBe(false);
  });

  it('is on for the Tizen dev shell previewed in a desktop browser', () => {
    stubUa(DESKTOP_UA);
    expect(computeEnv('Tizen').physicalKeyboard).toBe(true);
  });

  it('is off for a webOS build on a webOS TV, on in a desktop browser', () => {
    stubUa('Mozilla/5.0 (Web0S; Linux/SmartTV)');
    expect(computeEnv('webOS').physicalKeyboard).toBe(false);
    stubUa('Mozilla/5.0 (webOS; Linux/SmartTV)'); // the letter-O spelling counts too
    expect(computeEnv('webOS').physicalKeyboard).toBe(false);
    stubUa(DESKTOP_UA);
    expect(computeEnv('webOS').physicalKeyboard).toBe(true);
  });

  it('is off when only the webOS bridge global gives the TV away', () => {
    stubUa(DESKTOP_UA);
    vi.stubGlobal('webOS', { service: {} });
    expect(computeEnv('webOS').physicalKeyboard).toBe(false);
  });

  it('is off for the Android TV shell on Android', () => {
    stubUa('Mozilla/5.0 (Linux; Android 12; BRAVIA) wv');
    expect(computeEnv('Android TV').physicalKeyboard).toBe(false);
  });

  it('is on for the desktop shell and the generic browser platform', () => {
    stubUa(DESKTOP_UA);
    expect(computeEnv('Desktop').physicalKeyboard).toBe(true);
    expect(computeEnv('TV').physicalKeyboard).toBe(true);
  });

  it('honors an explicit override (Steam Deck: Desktop without a keyboard)', () => {
    stubUa(DESKTOP_UA);
    expect(computeEnv('Desktop', { physicalKeyboard: false }).physicalKeyboard).toBe(false);
    stubUa('Mozilla/5.0 (SMART-TV; LINUX; Tizen 6.0)');
    expect(computeEnv('Tizen', { physicalKeyboard: true }).physicalKeyboard).toBe(true);
  });

  it('keeps the OSK when navigator is unavailable on a TV shell', () => {
    vi.stubGlobal('navigator', undefined);
    expect(computeEnv('Tizen').physicalKeyboard).toBe(false);
  });
});

describe('computeEnv pointer flags', () => {
  it('mousePointer only on the desktop shell with a fine pointer', () => {
    stubUa(DESKTOP_UA);
    expect(computeEnv('Desktop', { pointer: true }).mousePointer).toBe(true);
    expect(computeEnv('webOS', { pointer: true }).mousePointer).toBe(false);
    expect(computeEnv('Desktop', { pointer: false }).mousePointer).toBe(false);
  });
});
