import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  formatBytes,
  formatDuration,
  formatHours,
  formatMbps,
  formatUptime,
  posterGradient,
  relativeSeen,
  timecode,
} from './adminFormat';

describe('posterGradient', () => {
  it('is deterministic and layers a radial + linear gradient', () => {
    const g = posterGradient('Inception');
    expect(g).toBe(posterGradient('Inception'));
    expect(g).toContain('radial-gradient(');
    expect(g).toContain('linear-gradient(155deg');
  });
});

describe('formatDuration', () => {
  it('formats watch time with a zero-padded minutes segment', () => {
    expect(formatDuration(0)).toBe('0 min');
    expect(formatDuration(65 * 60_000)).toBe('1 h 05 min');
    expect(formatDuration((4 * 60 + 29) * 60_000)).toBe('4 h 29 min');
  });
});

describe('formatHours', () => {
  it('renders hours with a French decimal comma', () => {
    expect(formatHours(14.3 * 3_600_000)).toBe('14,3 h');
    expect(formatHours(0)).toBe('0,0 h');
  });
});

describe('timecode', () => {
  it('drops the hour segment under one hour', () => {
    expect(timecode(0)).toBe('0:00');
    expect(timecode(8 * 60_000 + 30_000)).toBe('8:30');
    expect(timecode((3600 + 42 * 60 + 8) * 1000)).toBe('1:42:08');
  });
});

describe('formatMbps', () => {
  it('one decimal, French comma', () => {
    expect(formatMbps(5)).toBe('5,0');
    expect(formatMbps(12.34)).toBe('12,3');
  });
});

describe('formatUptime', () => {
  it('scales days / hours / minutes', () => {
    expect(formatUptime(8 * 60)).toBe('8 min');
    expect(formatUptime(4 * 3600 + 12 * 60)).toBe('4 h 12 min');
    expect(formatUptime(18 * 86400 + 4 * 3600)).toBe('18 j 04 h');
  });
});

describe('formatBytes (re-exported)', () => {
  it('picks the right unit', () => {
    expect(formatBytes(0)).toBe('0 o');
    expect(formatBytes(1536)).toBe('2 Ko');
  });
});

describe('relativeSeen', () => {
  afterEach(() => vi.useRealTimers());

  it('handles the null / unparseable cases', () => {
    expect(relativeSeen(null)).toBe('jamais');
    expect(relativeSeen(undefined)).toBe('jamais');
    expect(relativeSeen('not-a-date')).toBe('-');
  });

  it('renders a French relative label from an ISO timestamp', () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date('2024-06-15T12:00:00Z'));
    expect(relativeSeen('2024-06-15T11:59:30Z')).toBe("à l'instant");
    expect(relativeSeen('2024-06-15T11:55:00Z')).toBe('il y a 5 min');
    expect(relativeSeen('2024-06-15T09:00:00Z')).toBe('il y a 3 h');
    expect(relativeSeen('2024-06-14T11:00:00Z')).toBe('hier');
    expect(relativeSeen('2024-06-12T12:00:00Z')).toBe('il y a 3 j');
    // Older than 30 days falls back to an absolute (locale) date.
    expect(relativeSeen('2024-05-06T12:00:00Z')).toMatch(/\d/);
  });
});
