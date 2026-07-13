// Shared helpers for the module authoring scripts (gen / new / validate), so the
// reverse-DNS rule, frontmatter parsing, and name derivation live in ONE place
// instead of being copy-pasted (and drifting) across the three scripts.

/** Reverse-DNS id: at least two dot-separated lowercase segments (hyphens allowed
 *  after the first). Kept identical to `modules/module.schema.json`'s `id` pattern. */
export const REVERSE_DNS = /^[a-z0-9]+(\.[a-z0-9-]+)+$/;

export type Manifest = Record<string, unknown>;

/** Parse the YAML frontmatter of a `.module.md`, or null if it has none. */
export function frontmatter(md: string): Manifest | null {
  const m = md.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  return m ? (Bun.YAML.parse(m[1]) as Manifest) : null;
}

/** A fenced code block's contents by language (```lang ... ```), or null. */
export function fenced(md: string, lang: string): string | null {
  const m = md.match(new RegExp(`\`\`\`${lang}\\r?\\n([\\s\\S]*?)\\r?\\n\`\`\``));
  return m ? m[1] : null;
}

/** A reverse-DNS id -> crate / package / path slug (dots + non-alphanumerics -> '-'). */
export function slug(id: string): string {
  return id
    .replace(/[^a-z0-9]+/gi, '-')
    .replace(/^-+|-+$/g, '')
    .toLowerCase();
}
