// This shared helper lives at `clients/`, where the `vite` package isn't
// resolvable vite is a per-shell dependency (clients/tizen, clients/webos),
// not hoisted to the workspace root. So instead of importing vite's own `Plugin`
// type we describe just the slice we use; it stays structurally assignable to
// Vite's `PluginOption`, so `plugins: [tvFrame()]` type-checks in each config.
interface DevOnlyHtmlPlugin {
  name: string;
  apply: 'serve';
  transformIndexHtml: () => Array<{ tag: string; injectTo: 'head'; children: string }>;
}

export interface TvFrameOptions {
  /** Stage width in CSS px (the TV's logical canvas). Default 1920. */
  width?: number;
  /** Stage height in CSS px. Default 1080 (→ 16:9 with the default width). */
  height?: number;
  /** Turn the frame off without removing the plugin from the config. Default true. */
  enabled?: boolean;
}

/**
 * Dev-only **TV frame**. In `vite dev` it letterboxes the mounted app into a
 * fixed 16:9 stage (1920×1080 by default), scaled to fit the browser window
 * the way a real TV renders a 1080p canvas onto its panel. The shells ship a
 * `<meta name="viewport" width=1920 height=1080>` so a TV webview gets that exact
 * canvas, but desktop Chrome ignores that tag, so without this the TV UI just
 * stretches to whatever the browser window is. This restores the authored aspect.
 *
 * How it works: a `transform` on `#root` turns it into the containing block for
 * the app's `position: fixed` full-screen layers (TvHome / TvPlayer / detail /
 * profiles), so they fill the *stage* rather than the viewport. The injected
 * rules are unlayered, so they beat tv.css's `@layer base` `html/body/#root`
 * rules without `!important`.
 *
 * Never runs in `vite build` (`apply: 'serve'`), so production TV packages are
 * untouched. Press the ` (backtick) key to toggle framed / full-window; the
 * choice is remembered in localStorage.
 *
 * Caveat: a handful of `vh`-based `clamp()`s in the TV CSS still resolve against
 * the real window (CSS can't remap viewport units under a `transform`), so they
 * drift slightly on heavy up/down-scale. The fixed-px bulk of the layout which
 * is nearly all of it stays pixel-faithful.
 */
export function tvFrame(options: TvFrameOptions = {}): DevOnlyHtmlPlugin {
  const width = options.width ?? 1920;
  const height = options.height ?? 1080;
  const enabled = options.enabled ?? true;
  return {
    name: 'luma:tv-frame',
    apply: 'serve', // dev server only no effect on `vite build`
    transformIndexHtml() {
      if (!enabled) return [];
      return [
        { tag: 'style', injectTo: 'head', children: stageCss(width, height) },
        { tag: 'script', injectTo: 'head', children: stageJs(width, height) },
      ];
    },
  };
}

function stageCss(w: number, h: number): string {
  return `
/* LUMA dev TV frame injected by vite dev only (see clients/tv-frame.vite.ts) */
html[data-tv-frame="on"], html[data-tv-frame="on"] body {
  height: 100%; margin: 0; overflow: hidden;
  background: #0b0b0d; /* letterbox bars */
}
html[data-tv-frame="on"] #root {
  position: fixed; top: 50%; left: 50%;
  width: ${w}px; height: ${h}px;
  transform: translate(-50%, -50%) scale(var(--tv-frame-scale, 1));
  transform-origin: center center;
  overflow: hidden; background: #000;
  box-shadow: 0 0 0 1px rgba(255, 255, 255, 0.10), 0 30px 90px rgba(0, 0, 0, 0.7);
}
#luma-tv-frame-hint {
  position: fixed; right: 12px; bottom: 10px; z-index: 2147483647;
  font: 500 12px/1.4 ui-monospace, "SF Mono", monospace;
  color: rgba(255, 255, 255, 0.6); background: rgba(0, 0, 0, 0.5);
  padding: 4px 9px; border-radius: 7px; pointer-events: none;
  transition: opacity 0.4s ease;
}`;
}

function stageJs(w: number, h: number): string {
  // Classic (non-module) inline script in <head>: runs during head parse, before
  // the deferred app module mounts, so the stage is in place with no layout flash.
  return `(function () {
  var W = ${w}, H = ${h}, KEY = 'luma.tvFrame', root = document.documentElement;
  function fit() {
    root.style.setProperty('--tv-frame-scale', String(Math.min(innerWidth / W, innerHeight / H)));
  }
  var saved; try { saved = localStorage.getItem(KEY); } catch (e) {}
  var on = saved !== '0';
  var hint, hintTimer;
  function showHint() {
    if (!document.body) { document.addEventListener('DOMContentLoaded', showHint, { once: true }); return; }
    if (!hint) { hint = document.createElement('div'); hint.id = 'luma-tv-frame-hint'; document.body.appendChild(hint); }
    hint.textContent = on ? (W + '\\u00d7' + H + '  \\u00b7  \\u0060 to unframe') : ('full window  \\u00b7  \\u0060 to frame');
    hint.style.opacity = '1';
    clearTimeout(hintTimer);
    hintTimer = setTimeout(function () { if (hint) hint.style.opacity = '0'; }, 2600);
  }
  function set(v) {
    on = v;
    root.setAttribute('data-tv-frame', on ? 'on' : 'off');
    try { localStorage.setItem(KEY, on ? '1' : '0'); } catch (e) {}
    fit(); showHint();
  }
  root.setAttribute('data-tv-frame', on ? 'on' : 'off');
  fit();
  addEventListener('resize', fit, { passive: true });
  addEventListener('keydown', function (e) {
    var tag = ((e.target && e.target.tagName) || '').toUpperCase();
    if (e.key === '\\u0060' && tag !== 'INPUT' && tag !== 'TEXTAREA') {
      e.preventDefault(); e.stopImmediatePropagation(); set(!on);
    }
  }, true);
  showHint();
})();`;
}
