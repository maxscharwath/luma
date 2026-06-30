import { describe, expect, it } from 'vitest';
import { type AvplayTrack, audioAbsoluteIndex } from './engine';

describe('audioAbsoluteIndex (native AVPlay audio mapping)', () => {
  // AVPlay enumerates ALL streams; audio streams can sit at non-contiguous
  // absolute indices because video/text streams are interleaved.
  const streams: AvplayTrack[] = [
    { index: 1, type: 'AUDIO', extra_info: '{"language":"eng"}' },
    { index: 3, type: 'AUDIO', extra_info: '{"language":"fra"}' },
    { index: 4, type: 'AUDIO', extra_info: '{"language":"fra","channels":"2"}' },
  ];

  it('maps the audio-relative rendition to the absolute stream index', () => {
    expect(audioAbsoluteIndex(streams, 0)).toBe(1);
    expect(audioAbsoluteIndex(streams, 1)).toBe(3); // not 1: indices are interleaved
    expect(audioAbsoluteIndex(streams, 2)).toBe(4);
  });

  it('returns null for an out-of-range rendition', () => {
    expect(audioAbsoluteIndex(streams, 3)).toBeNull();
    expect(audioAbsoluteIndex([], 0)).toBeNull();
  });

  it('handles the contiguous case (absolute == relative)', () => {
    const contiguous: AvplayTrack[] = [
      { index: 0, type: 'AUDIO' },
      { index: 1, type: 'AUDIO' },
    ];
    expect(audioAbsoluteIndex(contiguous, 0)).toBe(0);
    expect(audioAbsoluteIndex(contiguous, 1)).toBe(1);
  });
});
