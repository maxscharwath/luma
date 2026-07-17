// Finalizes a dual-bundle TV package after the LEGACY vite build (which runs
// after the modern one - see the shell's package.json):
//
//  1. Post-processes <dist>/legacy/style.css: the kroma-legacy-css shims, then
//     @csstools/postcss-cascade-layers (compiles @layer away - old engines drop
//     unknown at-rules wholesale), then Lightning CSS down-level + minify for
//     the target Chrome floor. Done here, on the emitted file, so the
//     transforms always see Tailwind's final output regardless of plugin order.
//
//  2. Rewrites <dist>/index.html into an engine-gated loader: Chrome 99+ (has
//     CSSLayerBlockRule, the modern tier's real floor - Tailwind v4 keeps its
//     cascade layers there) loads the untouched ESM bundle; anything older
//     loads the flattened ES2015 IIFE bundle. One package serves every
//     generation.

import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import cascadeLayers from '@csstools/postcss-cascade-layers';
import { transform } from 'lightningcss';
import postcss from 'postcss';
import type { Plugin } from 'vite';
import { kromaLegacyCss } from './legacy-css';

async function downlevelCss(distDir: string, chrome: number): Promise<void> {
  const path = join(distDir, 'legacy', 'style.css');
  const raw = readFileSync(path, 'utf8');
  const shimmed = await postcss([kromaLegacyCss(), cascadeLayers()]).process(raw, {
    from: path,
    map: false,
  });
  const { code } = transform({
    filename: 'style.css',
    code: Buffer.from(shimmed.css),
    minify: true,
    targets: { chrome: chrome << 16 },
  });
  writeFileSync(path, code);
}

function rewriteIndexHtml(distDir: string): void {
  const path = join(distDir, 'index.html');
  let html = readFileSync(path, 'utf8');
  const js = /<script type="module"[^>]*src="([^"]+)"[^>]*><\/script>/.exec(html);
  const css = /<link rel="stylesheet"[^>]*href="([^"]+)"[^>]*>/.exec(html);
  if (!js || !css) {
    throw new Error(
      'legacy-finalize: modern <script type=module> / stylesheet not found in dist/index.html',
    );
  }
  // The loader itself must be ES5: it is the one script every engine parses.
  const loader = `<script>
      /* Engine gate: Chrome 99+ (cascade layers) takes the modern ESM bundle;
         older engines take the ES2015 legacy bundle. */
      (function () {
        var modern = typeof window.CSSLayerBlockRule !== 'undefined';
        var link = document.createElement('link');
        link.rel = 'stylesheet';
        link.href = modern ? '${css[1]}' : './legacy/style.css';
        document.head.appendChild(link);
        var script = document.createElement('script');
        if (modern) {
          script.type = 'module';
          script.crossOrigin = '';
          script.src = '${js[1]}';
        } else {
          script.src = './legacy/index.js';
        }
        document.body.appendChild(script);
      })();
    </script>`;
  html = html.replace(js[0], '').replace(css[0], '');
  html = html.replace('</body>', `${loader}\n  </body>`);
  writeFileSync(path, html);
}

/** `distDir` = the shell's absolute dist dir; `chrome` = the legacy tier's floor. */
export function legacyFinalize({ distDir, chrome }: { distDir: string; chrome: number }): Plugin {
  return {
    name: 'kroma-legacy-finalize',
    apply: 'build',
    enforce: 'post',
    async closeBundle() {
      await downlevelCss(distDir, chrome);
      rewriteIndexHtml(distDir);
    },
  };
}
