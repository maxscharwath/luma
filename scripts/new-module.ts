#!/usr/bin/env bun
// Scaffold a new single-file module: `bun run modules:new <reverse.dns.id>`.
// Writes a starter modules/<slug>.module.md; run `bun run modules:gen` after.

import { existsSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { REVERSE_DNS, slug as toSlug } from './module-format';

const id = process.argv[2];
if (!id || !REVERSE_DNS.test(id)) {
  console.error('usage: bun run modules:new <reverse.dns.id>   e.g. bun run modules:new dev.luma.notes');
  process.exit(1);
}

const slug = toSlug(id);
const file = join(import.meta.dir, '..', 'modules', `${slug}.module.md`);
if (existsSync(file)) {
  console.error(`already exists: modules/${slug}.module.md`);
  process.exit(1);
}

const leaf = id.split('.').pop() ?? id;
const title = leaf.charAt(0).toUpperCase() + leaf.slice(1);

const template = `---
id: ${id}
name: ${title}
version: 0.1.0
description: "TODO: one-line description of ${title}."
dependsOn: []
provides: []
permissions:
  - library.manage
config: []
---

# ${title}

Authored as one file. Run \`bun run modules:gen\` after editing. The backend
registry entry (\`pub const MODULE\`) is generated from this manifest; add an
optional \`\`\`rust block only to contribute extra backend items alongside it (do
not redefine \`MODULE\`). Drop an \`\`\`svg block to package an icon.

\`\`\`tsx
import type { LumaModule, ModuleComponentProps } from '@luma/module-sdk';
import manifest from '../../module.json';

function Panel(_: ModuleComponentProps) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-lg font-semibold text-text">{manifest.name}</h2>
      <p className="text-sm text-muted">{manifest.description}</p>
    </section>
  );
}

export const module: LumaModule = {
  id: manifest.id,
  version: manifest.version,
  dependsOn: manifest.dependsOn,
  // A user-facing page at /m/${slug}. For an admin-only page instead, use
  // section: 'admin' and to: '/admin/m/${slug}'.
  navItems: [{ to: '/m/${slug}', label: manifest.name, section: 'library' }],
  routes: [{ path: '${slug}', component: Panel }],
};
\`\`\`
`;

writeFileSync(file, template);
console.log(`created modules/${slug}.module.md - edit it, then run \`bun run modules:gen\``);
