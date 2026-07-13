#!/usr/bin/env bun
// Validate every module manifest against modules/module.schema.json:
//   - each server/modules/<id>/module.json  (compiled-in modules)
//   - each wasm-modules/<id>/module.json     (runtime-loaded modules)
//   - the YAML frontmatter of each modules/*.module.md source
// Enforces the reverse-DNS `id` pattern (among the rest of the schema). Exits
// non-zero with a report on any violation, so it can gate CI / the build.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { frontmatter } from './module-format';

const ROOT = join(import.meta.dir, '..');
const schema = JSON.parse(readFileSync(join(ROOT, 'modules', 'module.schema.json'), 'utf8'));

type Json = Record<string, unknown>;

/** Minimal JSON Schema (draft-07 subset) validator: the keywords our schema
 *  uses - type, required, properties, additionalProperties, pattern, minLength,
 *  enum, items. */
function validate(node: Json, value: unknown, path: string, errors: string[]): void {
  // `oneOf`: the value must validate against exactly one alternative (we only
  // need "at least one" for our permissive subset, e.g. dependsOn = object|array).
  if (Array.isArray(node.oneOf)) {
    const ok = (node.oneOf as Json[]).some((sub) => {
      const subErrors: string[] = [];
      validate(sub, value, path, subErrors);
      return subErrors.length === 0;
    });
    if (!ok) errors.push(`${path}: does not match any of the allowed forms`);
    return;
  }
  const type = node.type as string | undefined;
  if (type === 'object') {
    if (typeof value !== 'object' || value === null || Array.isArray(value)) {
      errors.push(`${path}: expected object`);
      return;
    }
    const obj = value as Json;
    const props = (node.properties ?? {}) as Record<string, Json>;
    for (const req of (node.required ?? []) as string[]) {
      if (!(req in obj)) errors.push(`${path}: missing required "${req}"`);
    }
    if (node.additionalProperties === false) {
      for (const key of Object.keys(obj)) {
        if (!(key in props)) errors.push(`${path}: unknown property "${key}"`);
      }
    }
    // additionalProperties as a schema: validate every value not named in
    // `properties` against it, so the dependsOn `{ id: range }` map rejects the
    // same non-string ranges the Rust loader (next_entry::<String, String>) does.
    if (node.additionalProperties && typeof node.additionalProperties === 'object') {
      for (const [key, val] of Object.entries(obj)) {
        if (!(key in props))
          validate(node.additionalProperties as Json, val, `${path}.${key}`, errors);
      }
    }
    for (const [key, sub] of Object.entries(props)) {
      if (key in obj) validate(sub, obj[key], `${path}.${key}`, errors);
    }
  } else if (type === 'array') {
    if (!Array.isArray(value)) {
      errors.push(`${path}: expected array`);
      return;
    }
    // A permissive array (no `items` schema, e.g. the mixed-form dependsOn) skips
    // per-item validation.
    const items = node.items as Json | undefined;
    if (items)
      value.forEach((item, i) => {
        validate(items, item, `${path}[${i}]`, errors);
      });
  } else if (type === 'string') {
    if (typeof value !== 'string') {
      errors.push(`${path}: expected string`);
      return;
    }
    if (typeof node.minLength === 'number' && value.length < node.minLength) {
      errors.push(`${path}: must not be empty`);
    }
    if (typeof node.pattern === 'string' && !new RegExp(node.pattern).test(value)) {
      errors.push(`${path}: "${value}" does not match ${node.pattern}`);
    }
  } else if (type === 'boolean') {
    if (typeof value !== 'boolean') errors.push(`${path}: expected boolean`);
  } else if (type === 'number' || type === 'integer') {
    if (typeof value !== 'number' || Number.isNaN(value)) errors.push(`${path}: expected number`);
    else if (type === 'integer' && !Number.isInteger(value))
      errors.push(`${path}: expected integer`);
  }
  if (Array.isArray(node.enum) && !node.enum.includes(value)) {
    errors.push(`${path}: ${JSON.stringify(value)} not one of ${JSON.stringify(node.enum)}`);
  }
}

const errors: string[] = [];

/** List a directory (sorted for stable output), returning [] when absent. */
function optionalReaddir(dir: string): string[] {
  try {
    return readdirSync(dir).sort();
  } catch {
    return [];
  }
}

// Manifest ids seen among the compiled (server/modules) + runtime (wasm-modules)
// sets: an id present in BOTH is a real collision (a runtime WASM module would
// shadow a built-in and share its enabled flag). Single-file sources are their
// own generated mirror, so they are checked at codegen time, not here.
const compiledWasmIds: { id: string; label: string }[] = [];

/** Read + parse + schema-check one `module.json`, skipping absent dirs. */
function validateManifestFile(manifestPath: string, label: string): void {
  try {
    if (!statSync(manifestPath).isFile()) return;
  } catch {
    return;
  }
  let value: unknown;
  try {
    value = JSON.parse(readFileSync(manifestPath, 'utf8'));
  } catch (e) {
    errors.push(`${label}: invalid JSON (${(e as Error).message})`);
    return;
  }
  validate(schema, value, label, errors);
  const id = (value as Json)?.id;
  if (typeof id === 'string') compiledWasmIds.push({ id, label });
}

// 1) Compiled-in module manifests + 2) runtime (WASM) module manifests.
for (const [root, prefix] of [
  [join(ROOT, 'server', 'modules'), 'server/modules'],
  [join(ROOT, 'wasm-modules'), 'wasm-modules'],
] as const) {
  for (const id of optionalReaddir(root)) {
    validateManifestFile(join(root, id, 'module.json'), `${prefix}/${id}/module.json`);
  }
}

// 3) Single-file sources (the frontmatter is the manifest).
const srcDir = join(ROOT, 'modules');
for (const file of optionalReaddir(srcDir).filter((f) => f.endsWith('.module.md'))) {
  const fm = frontmatter(readFileSync(join(srcDir, file), 'utf8'));
  if (!fm) {
    errors.push(`modules/${file}: missing YAML frontmatter`);
    continue;
  }
  validate(schema, fm, `modules/${file}`, errors);
}

// Report any id that appears in more than one of the compiled/WASM manifests.
const byId = new Map<string, string[]>();
for (const { id, label } of compiledWasmIds) {
  const labels = byId.get(id) ?? [];
  labels.push(label);
  byId.set(id, labels);
}
for (const [id, labels] of byId) {
  if (labels.length > 1) errors.push(`duplicate module id "${id}" in: ${labels.join(', ')}`);
}

if (errors.length) {
  console.error(`module manifest validation failed (${errors.length}):`);
  for (const e of errors) console.error(`  - ${e}`);
  process.exit(1);
}
console.log('all module manifests valid');
