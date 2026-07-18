import { afterEach, describe, expect, it, vi } from 'vitest';
import { clamp01, endsAtClock, pct, sliderToVolume, volumeToSlider } from './fmt';

afterEach(() => vi.useRealTimers());

describe('endsAtClock', () => {
  it('returns an empty string for missing / non-positive runtimes', () => {
    expect(endsAtClock(null)).toBe('');
    expect(endsAtClock(undefined)).toBe('');
    expect(endsAtClock(0)).toBe('');
    expect(endsAtClock(-5000)).toBe('');
  });

  it('formats the fr 24h "22h38" clock, zero-padding minutes', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T12:00:00Z'));
    const remainingMs = 42 * 60_000; // 42 minutes from now
    // Derive expectation from the same local-time construction so the assertion
    // is timezone-independent (getHours/getMinutes are local on both sides).
    const d = new Date(Date.now() + remainingMs);
    const expected = `${d.getHours()}h${String(d.getMinutes()).padStart(2, '0')}`;
    expect(endsAtClock(remainingMs)).toBe(expected);
    // The fr branch is also the default when no locale is given.
    expect(endsAtClock(remainingMs, 'fr')).toBe(expected);
    // Minutes below 10 are zero-padded.
    expect(/h\d{2}$/.test(endsAtClock(remainingMs))).toBe(true);
  });

  it('formats the en locale as a 12h AM/PM clock', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2026-01-01T12:00:00Z'));
    const out = endsAtClock(90 * 60_000, 'en');
    expect(out).toMatch(/^\d{1,2}:\d{2}\s?[AP]M$/i);
  });
});

describe('clamp01', () => {
  it('passes through values already inside [0, 1]', () => {
    expect(clamp01(0)).toBe(0);
    expect(clamp01(0.5)).toBe(0.5);
    expect(clamp01(1)).toBe(1);
  });

  it('clamps out-of-range values to the nearest bound', () => {
    expect(clamp01(-3)).toBe(0);
    expect(clamp01(2.7)).toBe(1);
  });
});

describe('pct', () => {
  it('returns the clamped percentage of value within total', () => {
    expect(pct(1, 4)).toBe(25);
    expect(pct(2, 2)).toBe(100);
    expect(pct(0, 10)).toBe(0);
  });

  it('clamps a value beyond total to 100', () => {
    expect(pct(15, 10)).toBe(100);
  });

  it('is safe (0) when total is zero or negative', () => {
    expect(pct(5, 0)).toBe(0);
    expect(pct(5, -1)).toBe(0);
  });
});

describe('perceptual volume curve', () => {
  it('pins the endpoints and tapers the middle below linear', () => {
    expect(sliderToVolume(0)).toBe(0);
    expect(sliderToVolume(1)).toBe(1);
    // A centred slider yields a much quieter amplitude than a linear 0.5.
    expect(sliderToVolume(0.5)).toBeCloseTo(0.125, 5);
  });

  it('round-trips through the inverse', () => {
    for (const v of [0, 0.125, 0.4, 0.8, 1]) {
      expect(volumeToSlider(sliderToVolume(v))).toBeCloseTo(v, 5);
    }
    expect(volumeToSlider(0.125)).toBeCloseTo(0.5, 5);
  });

  it('clamps out-of-range inputs', () => {
    expect(sliderToVolume(-1)).toBe(0);
    expect(sliderToVolume(2)).toBe(1);
    expect(volumeToSlider(-1)).toBe(0);
    expect(volumeToSlider(2)).toBe(1);
  });
});
