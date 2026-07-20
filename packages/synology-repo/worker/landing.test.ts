import { describe, expect, it } from 'vitest';
import type { Catalog, Entry } from './catalog';
import { renderLanding } from './landing';

function entry(over: Partial<Entry> = {}): Entry {
  return {
    channel: 'stable',
    tag: 'v1.0.0',
    releaseName: 'KROMA 1.0.0',
    releaseUrl: 'https://gh/rel/1.0.0',
    publishedAt: '2026-07-01T12:00:00Z',
    spkName: 'kroma-1.0.0-abc-x86_64.spk',
    spkUrl: 'https://dl/kroma-1.0.0.spk',
    spkSize: 2097152,
    info: null,
    ...over,
  };
}

const catalog = (entries: Entry[]): Catalog => ({
  fetchedAt: '2026-07-02T00:00:00Z',
  repo: 'maxscharwath/kroma',
  entries,
});

describe('renderLanding branding', () => {
  // Regression: the mark used to be fetched from /icon.png, which the worker
  // proxied from the repo behind a 24h edge cache. That kept serving the
  // pre-rebrand logo long after the assets changed, so it is now bundled.
  it('inlines the brand mark instead of fetching /icon.png', () => {
    const html = renderLanding(catalog([entry()]), 'https://pkg.kroma.tv');
    expect(html).toMatch(/rel="icon" href="data:image\/svg\+xml/);
    expect(html).toMatch(/<h1><svg[^>]*aria-label="KROMA"/);
    expect(html).not.toContain('/icon.png');
  });
});

describe('renderLanding', () => {
  it('renders the package-source URL, version, size and a row per entry', () => {
    const html = renderLanding(catalog([entry()]), 'https://pkg.kroma.tv');
    expect(html).toContain('<code class="url">https://pkg.kroma.tv/</code>');
    expect(html).toContain('1.0.0-abc'); // version derived from spk name
    expect(html).toContain('2.0 MB'); // 2097152 bytes
    expect(html).toContain('2026-07-01'); // published day only
    expect(html).toContain('https://dl/kroma-1.0.0.spk');
    expect(html).toContain('Latest stable');
  });

  it('shows the empty-state when there is no stable release', () => {
    const html = renderLanding(catalog([entry({ channel: 'nightly' })]), 'https://pkg.kroma.tv');
    expect(html).toContain('No stable release published yet.');
    expect(html).toContain('<h2>Nightly</h2>');
    expect(html).toContain('nightly'); // channel tag on the row
  });

  it('omits the nightly section when there is no nightly entry', () => {
    const html = renderLanding(catalog([entry()]), 'https://pkg.kroma.tv');
    expect(html).not.toContain('<h2>Nightly</h2>');
  });

  it('prefers sidecar size when present', () => {
    const withInfo = entry({
      info: {
        package: 'kroma',
        version: '1.0.0',
        dname: 'KROMA',
        desc: 'd',
        arch: 'x86_64',
        firmware: '7.0-40000',
        size: 5242880, // 5 MB
        md5: 'x',
        beta: false,
      },
    });
    const html = renderLanding(catalog([withInfo]), 'https://pkg.kroma.tv');
    expect(html).toContain('5.0 MB');
    expect(html).toContain('>1.0.0<'); // sidecar version
  });

  it('HTML-escapes untrusted-looking version/url text', () => {
    const evil = entry({
      info: {
        package: 'kroma',
        version: '<b>&"',
        dname: 'KROMA',
        desc: 'd',
        arch: 'x86_64',
        firmware: '7.0',
        size: 1,
        md5: 'x',
        beta: false,
      },
    });
    const html = renderLanding(catalog([evil]), 'https://pkg.kroma.tv');
    expect(html).toContain('&lt;b&gt;&amp;&quot;');
    expect(html).not.toContain('<b>&"');
  });

  it('includes the repo footer and refreshed timestamp', () => {
    const html = renderLanding(catalog([entry()]), 'https://pkg.kroma.tv');
    expect(html).toContain('github.com/maxscharwath/kroma/releases');
    expect(html).toContain('2026-07-02T00:00:00Z');
  });
});
