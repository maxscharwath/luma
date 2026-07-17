// Legacy-engine CSS compat (old webOS: Chromium 53-94). A PostCSS plugin that
// rewrites the modern CSS Tailwind v4 emits into equivalents those engines
// execute. It runs BEFORE @csstools/postcss-cascade-layers (which then compiles
// the @layer blocks away), so the rules it inserts inherit the correct layer /
// order semantics:
//
//  - flex `gap` (Chrome 84) -> the negative-margin technique: the container
//    pulls -gap/2 per axis, every child pushes +gap/2, so spacing, wrapping and
//    edge alignment all match real gap.
//  - `aspect-ratio` (Chrome 88) -> a `::before` strut with percentage
//    padding-bottom (resolves against width), which also centres in-flow
//    children of flex tiles the way aspect-ratio does.
//  - `scale:` / `translate:` properties (Chrome 104) -> one composed
//    `transform`, the two utilities cooperating through the same --tw-* vars.
//  - unwraps Tailwind's @supports fallback that seeds the @property --tw-*
//    initial values: its Safari/Firefox-shaped condition never matches old
//    Chromium, but the plain rules inside are exactly what it needs.
//  - margin utilities are re-appended after the generated child-margin rules so
//    an explicit `ml-auto` / `m-*` on a gap child still wins the cascade.

import type { AtRule, ChildNode, Declaration, Plugin, Root, Rule } from 'postcss';

const RATIO = /(\d+(?:\.\d+)?)\s*\/\s*(\d+(?:\.\d+)?)/;

/** Split a CSS value on top-level whitespace (never inside parentheses). */
function splitSpace(value: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let cur = '';
  for (const ch of value.trim()) {
    if (ch === '(') depth += 1;
    if (ch === ')') depth -= 1;
    if (/\s/.test(ch) && depth === 0) {
      if (cur) parts.push(cur);
      cur = '';
    } else {
      cur += ch;
    }
  }
  if (cur) parts.push(cur);
  return parts;
}

/** Half of a CSS length as a calc(), sign '' (positive) or '-' (negative). */
function half(value: string, sign: '' | '-'): string {
  const inner = value.startsWith('calc(') && value.endsWith(')') ? value.slice(5, -1) : value;
  return `calc((${inner}) * ${sign}.5)`;
}

/** All `--aspect-*`-style custom properties whose value is a `W / H` ratio. */
function collectRatioVars(root: Root): Map<string, string> {
  const map = new Map<string, string>();
  root.walkDecls(/^--/, (d) => {
    if (RATIO.test(d.value)) map.set(d.prop, d.value);
  });
  return map;
}

/** `aspect-ratio: W/H` -> `S::before { padding-bottom: H/W% }` strut. */
function shimAspect(root: Root, ratios: Map<string, string>): void {
  const decls: Declaration[] = [];
  root.walkDecls('aspect-ratio', (d) => {
    decls.push(d);
  });
  for (const decl of decls) {
    const rule = decl.parent;
    if (rule?.type !== 'rule') continue;
    let raw = decl.value.trim();
    const viaVar = /^var\((--[\w-]+)\)$/.exec(raw);
    if (viaVar) raw = ratios.get(viaVar[1] ?? '') ?? raw;
    const m = RATIO.exec(raw) ?? (/^\d+(?:\.\d+)?$/.test(raw) ? [raw, raw, '1'] : null);
    if (!m) continue; // unresolvable ratio: leave it (the compat check will flag it)
    const pct = Math.round((Number(m[2]) / Number(m[1])) * 10000) / 100;
    const strut = rule.cloneAfter({
      selectors: rule.selectors.map((s) => `${s}::before`),
    });
    strut.removeAll();
    strut.append(
      { prop: 'content', value: '""' },
      { prop: 'display', value: 'block' },
      { prop: 'padding-bottom', value: `${pct}%` },
    );
    decl.remove();
    if (rule.nodes.length === 0) rule.remove();
  }
}

/** flex `gap` -> container -gap/2 margins + a `S > *` child rule with +gap/2. */
function shimGap(root: Root, generated: WeakSet<ChildNode>): void {
  const decls: Declaration[] = [];
  root.walkDecls(/^(gap|column-gap|row-gap)$/, (d) => {
    decls.push(d);
  });
  for (const decl of decls) {
    const rule = decl.parent;
    if (rule?.type !== 'rule') continue;
    const parts = splitSpace(decl.value);
    const rowV = parts[0] ?? decl.value;
    const colV = decl.prop === 'gap' ? (parts[1] ?? rowV) : rowV;
    const container: Array<{ prop: string; value: string }> = [];
    const child: Array<{ prop: string; value: string }> = [];
    if (decl.prop !== 'column-gap') {
      container.push(
        { prop: 'margin-top', value: half(rowV, '-') },
        { prop: 'margin-bottom', value: half(rowV, '-') },
      );
      child.push(
        { prop: 'margin-top', value: half(rowV, '') },
        { prop: 'margin-bottom', value: half(rowV, '') },
      );
    }
    if (decl.prop !== 'row-gap') {
      container.push(
        { prop: 'margin-left', value: half(colV, '-') },
        { prop: 'margin-right', value: half(colV, '-') },
      );
      child.push(
        { prop: 'margin-left', value: half(colV, '') },
        { prop: 'margin-right', value: half(colV, '') },
      );
    }
    const childRule = rule.cloneAfter({
      selectors: rule.selectors.map((s) => `${s} > *`),
    });
    childRule.removeAll();
    childRule.append(...child);
    generated.add(childRule);
    generated.add(rule);
    decl.replaceWith(...container);
  }
}

/** `scale:` / `translate:` properties -> one composed `transform`. Both rules
 * emit the SAME transform value, so whichever wins the cascade still composes
 * the other utility's `--tw-*` variables (with inline fallbacks). */
function shimScaleTranslate(root: Root): void {
  const COMPOSED =
    'translate(var(--tw-translate-x, 0), var(--tw-translate-y, 0)) ' +
    'scale(var(--tw-scale-x, 1), var(--tw-scale-y, 1))';
  const inKeyframes = (d: Declaration): boolean => {
    for (let p = d.parent; p; p = p.parent as Declaration['parent']) {
      if (p.type === 'atrule' && /keyframes/i.test((p as AtRule).name)) return true;
    }
    return false;
  };
  const scales: Declaration[] = [];
  const translates: Declaration[] = [];
  root.walkDecls('scale', (d) => {
    if (!inKeyframes(d)) scales.push(d);
  });
  root.walkDecls('translate', (d) => {
    if (!inKeyframes(d)) translates.push(d);
  });
  for (const d of scales) {
    const repl: Array<{ prop: string; value: string }> = [{ prop: 'transform', value: COMPOSED }];
    if (!d.value.includes('var(')) {
      const [sx = '1', sy = sx] = splitSpace(d.value);
      repl.unshift({ prop: '--tw-scale-x', value: sx }, { prop: '--tw-scale-y', value: sy });
    }
    d.replaceWith(...repl);
  }
  for (const d of translates) {
    const repl: Array<{ prop: string; value: string }> = [{ prop: 'transform', value: COMPOSED }];
    if (!d.value.includes('var(')) {
      const [tx = '0', ty = '0'] = splitSpace(d.value);
      repl.unshift(
        { prop: '--tw-translate-x', value: tx },
        { prop: '--tw-translate-y', value: ty },
      );
    }
    d.replaceWith(...repl);
  }
}

/** Drop bare `display: grid|inline-grid` utilities. Chromium 53 ignores the
 * declaration anyway (the element stays block), so removing it just makes the
 * 87/94 legacy engines behave identically. Real grid LAYOUTS (grid-template*,
 * col-span, ...) are not silently fixed - the compat check fails the build. */
function stripGridDisplay(root: Root): void {
  const decls: Declaration[] = [];
  root.walkDecls('display', (d) => {
    if (/^(inline-)?grid$/.test(d.value.trim())) decls.push(d);
  });
  for (const d of decls) {
    const rule = d.parent;
    d.remove();
    if (rule?.nodes.length === 0) rule.remove();
  }
}

/** Unwrap Tailwind's `@supports` fallback carrying the @property --tw-* initial
 * values (recognised by its `-moz-orient` probe): old Chromium fails the
 * condition but needs exactly the rules inside it. */
function unwrapPropertyFallback(root: Root): void {
  const targets: AtRule[] = [];
  root.walkAtRules('supports', (at) => {
    if (at.params.includes('-moz-orient')) targets.push(at);
  });
  for (const at of targets) at.replaceWith(at.nodes ?? []);
}

/** Move margin-only utility rules to the end of the utilities layer, after the
 * generated `S > *` gap-child margins, so explicit margins keep winning. */
function hoistMarginUtilities(root: Root, generated: WeakSet<ChildNode>): void {
  root.walkAtRules('layer', (at) => {
    if (at.params !== 'utilities' || !at.nodes) return;
    const movers: Rule[] = [];
    at.each((node) => {
      if (node.type !== 'rule' || generated.has(node)) return;
      const decls = node.nodes ? node.nodes.filter((n) => n.type === 'decl') : [];
      if (decls.length && decls.every((d) => d.prop.startsWith('margin'))) {
        movers.push(node);
      }
    });
    for (const r of movers) {
      r.remove();
      at.append(r);
    }
  });
}

export function kromaLegacyCss(): Plugin {
  return {
    postcssPlugin: 'kroma-legacy-css',
    Once(root) {
      const generated = new WeakSet<ChildNode>();
      unwrapPropertyFallback(root);
      stripGridDisplay(root);
      shimAspect(root, collectRatioVars(root));
      shimGap(root, generated);
      shimScaleTranslate(root);
      hoistMarginUtilities(root, generated);
    },
  };
}
