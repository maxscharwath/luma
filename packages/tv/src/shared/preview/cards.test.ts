import type { ContinueItem, KromaClient, MediaItem } from '@kroma/core';
import { describe, expect, it } from 'vitest';
import { buildPreviewData } from './cards';

// buildPreviewData reads only `client.baseUrl`.
const client = { baseUrl: 'http://kroma.test' } as unknown as KromaClient;

function mov(o: Partial<MediaItem>): MediaItem {
  return {
    id: 'i1',
    title: 'Movie',
    kind: 'movie',
    addedAt: '2024-01-01T00:00:00Z',
    year: 2024,
    durationMs: 0,
    metadata: { backdropUrl: '/api/b.webp' },
    ...o,
  } as unknown as MediaItem;
}

// biome-ignore lint/suspicious/noExplicitAny: parsed JSON is dynamic
type Doc = { sections: Array<{ title: string; tiles: any[] }> };
const parse = (s: string | null): Doc => JSON.parse(s as string) as Doc;

describe('buildPreviewData', () => {
  it('returns null when nothing has art', () => {
    expect(buildPreviewData(client, [mov({ metadata: null })], [])).toBeNull();
    expect(buildPreviewData(client, [])).toBeNull();
  });

  it('builds a recent section, newest addedAt first', () => {
    const out = parse(
      buildPreviewData(client, [
        mov({ id: 'a', addedAt: '2024-01-01T00:00:00Z', title: 'Old' }),
        mov({ id: 'b', addedAt: '2024-05-01T00:00:00Z', title: 'New' }),
      ]),
    );
    expect(out.sections).toHaveLength(1);
    expect(out.sections[0]?.title).toBe('Ajout récent');
    expect(out.sections[0]?.tiles.map((t) => t.title)).toEqual(['New', 'Old']);
    const tile = out.sections[0]?.tiles[0];
    expect(tile.is_playable).toBe(false);
    expect(tile.image_ratio).toBe('16by9');
    expect(tile.image_url).toContain('http://kroma.test/api/items/b/card?');
    expect(tile.image_url).toContain('label=Nouveaut%C3%A9');
    expect(JSON.parse(tile.action_data)).toEqual({ type: 'movie', id: 'b' });
  });

  it('excludes items without art from the recent row', () => {
    const out = parse(
      buildPreviewData(client, [mov({ id: 'a' }), mov({ id: 'b', metadata: null })]),
    );
    expect(out.sections[0]?.tiles).toHaveLength(1);
    expect(out.sections[0]?.tiles[0].image_url).toContain('/items/a/');
  });

  it('caps each row at 20 tiles', () => {
    const many = Array.from({ length: 30 }, (_, i) =>
      mov({ id: `m${i}`, addedAt: `2024-01-${String((i % 28) + 1).padStart(2, '0')}T00:00:00Z` }),
    );
    const out = parse(buildPreviewData(client, many));
    expect(out.sections[0]?.tiles).toHaveLength(20);
  });

  it('builds a resume row first, with a progress fraction', () => {
    const cont: ContinueItem[] = [
      { item: mov({ id: 'r1' }), positionMs: 300, durationMs: 1000 } as unknown as ContinueItem,
    ];
    const out = parse(buildPreviewData(client, [mov({ id: 'a' })], cont));
    expect(out.sections[0]?.title).toBe('Reprendre la lecture');
    expect(out.sections[1]?.title).toBe('Ajout récent');
    expect(out.sections[0]?.tiles[0].image_url).toContain('progress=0.300');
    expect(out.sections[0]?.tiles[0].image_url).toContain('label=Reprendre');
  });

  it('omits the progress param when there is no duration', () => {
    const cont: ContinueItem[] = [
      { item: mov({ id: 'r1' }), positionMs: 300, durationMs: null } as unknown as ContinueItem,
    ];
    const out = parse(buildPreviewData(client, [], cont));
    expect(out.sections[0]?.tiles[0].image_url).not.toContain('progress=');
  });

  it('points an episode tile at its show', () => {
    const out = parse(
      buildPreviewData(client, [
        mov({ id: 'e1', kind: 'episode', showId: 's9', showTitle: 'Show' }),
      ]),
    );
    const tile = out.sections[0]?.tiles[0];
    expect(tile.title).toBe('Show');
    expect(JSON.parse(tile.action_data)).toEqual({ type: 'show', id: 's9' });
    expect(tile.subtitle.startsWith('Série')).toBe(true);
  });
});
