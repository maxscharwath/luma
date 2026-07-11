---
id: dev.luma.hello
name: Hello
version: 0.1.0
description: "A single-file demo module: manifest + packaged icon + React frontend in one file."
dependsOn: []
provides:
  - kind: demo
    id: greeting
permissions:
  - library.manage
config:
  - key: greeting
    label: Greeting text
    type: string
    default: Hello
---

# Single-file module

Authored as ONE file. The YAML frontmatter is the manifest (id must be
reverse-DNS), the `svg` block becomes the packaged `icon.svg` next to
`module.json`, and the `tsx` block is the frontend. The backend `Plugin` is
generated for you (add an optional `rust` block only for extra backend items).
Run `bun run modules:gen`; it also updates the aggregator rosters.

The packaged icon (written to `server/modules/<id>/icon.svg`, served at
`GET /api/modules/<id>/icon`):

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#c084fc"><path d="M12 2l2 6 6 2-6 2-2 6-2-6-6-2 6-2z"/></svg>
```

The frontend (becomes `fe/src/index.tsx`):

```tsx
import type { LumaModule, ModuleComponentProps } from '@luma/module-sdk';
import manifest from '../../module.json';

function HelloPanel({ host }: ModuleComponentProps) {
  return (
    <section className="flex flex-col gap-2">
      <h2 className="text-lg font-semibold text-text">Hello from a single-file module</h2>
      <p className="text-sm text-muted">
        Authored in one file, expanded by the module codegen. Its icon is a real
        icon.svg served from the backend.
      </p>
      <p className="text-xs text-dim">
        module id: {manifest.id} · host locale: {host.i18n.locale}
      </p>
    </section>
  );
}

export const module: LumaModule = {
  id: manifest.id,
  version: manifest.version,
  dependsOn: manifest.dependsOn,
  navItems: [{ to: '/admin/m/hello', label: manifest.name, section: 'admin' }],
  routes: [{ path: 'hello', component: HelloPanel }],
};
```

Optional migrations (becomes `be/migrations.sql`):

```sql
CREATE TABLE IF NOT EXISTS hello_log (id INTEGER PRIMARY KEY, at TEXT NOT NULL);
```
