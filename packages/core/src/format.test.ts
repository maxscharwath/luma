import { describe, expect, it } from 'vitest';
import { episodeTag, metaLine, playerSubtitle } from './format';
import type { MediaItem } from '@kroma/client';

// ----- fixtures --------------------------------------------------------------

function episode(p: {
  season?: number | null;
  episode?: number | null;
  episodeEnd?: number | null;
  showTitle?: string | null;
  title?: string;
}): MediaItem {
  return {
    kind: 'episode',
    title: p.title ?? 'Pilot',
    showTitle: 'showTitle' in p ? p.showTitle : 'The Show',
    season: 'season' in p ? p.season : 1,
    episode: 'episode' in p ? p.episode : 5,
    episodeEnd: p.episodeEnd ?? null,
  } as unknown as MediaItem;
}

const MOVIE = {
  kind: 'movie',
  title: 'Blade Runner',
  year: 1982,
  durationMs: 7620000,
  video: { codec: 'hevc', width: 3840, hdr: true },
} as unknown as MediaItem;

// ----- episodeTag ------------------------------------------------------------

describe('episodeTag', () => {
  it('zero-pads season and episode', () => {
    expect(episodeTag(episode({ season: 1, episode: 5 }))).toBe('S01E05');
    expect(episodeTag(episode({ season: 12, episode: 34 }))).toBe('S12E34');
  });

  it('renders a range for a multi-episode file', () => {
    expect(episodeTag(episode({ season: 1, episode: 5, episodeEnd: 6 }))).toBe('S01E05-E06');
  });

  it('ignores a degenerate episodeEnd (<= episode)', () => {
    expect(episodeTag(episode({ season: 2, episode: 3, episodeEnd: 3 }))).toBe('S02E03');
  });

  it('returns an empty string when unnumbered', () => {
    expect(episodeTag(episode({ season: null, episode: 5 }))).toBe('');
    expect(episodeTag(episode({ season: 1, episode: null }))).toBe('');
  });
});

// ----- playerSubtitle --------------------------------------------------------

describe('playerSubtitle', () => {
  it('joins show title and the S/E tag for an episode', () => {
    expect(playerSubtitle(episode({ showTitle: 'Severance', season: 1, episode: 5 }))).toBe(
      'Severance · S01E05',
    );
  });

  it('falls back to just the tag when the show title is missing', () => {
    expect(playerSubtitle(episode({ showTitle: null, season: 3, episode: 2 }))).toBe('S03E02');
  });

  it('uses the movie meta line for non-episodes', () => {
    expect(playerSubtitle(MOVIE)).toBe(metaLine(MOVIE));
  });
});
