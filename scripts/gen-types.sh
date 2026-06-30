#!/usr/bin/env bash
# Regenerates the TypeScript wire types in packages/core/src/generated/ from the
# Rust server's #[derive(TS)] structs (the single source of truth). Run after
# changing any annotated type in server/src. CI runs this and fails if the result
# drifts from the committed files (see .github/workflows/verify.yml).
set -euo pipefail
cd "$(dirname "$0")/.."
GEN_DIR="packages/core/src/generated"

# Re-export from scratch so a removed Rust type drops its stale .ts file too.
rm -rf "$GEN_DIR"
mkdir -p "$GEN_DIR"
( cd server && TS_RS_EXPORT_DIR="../$GEN_DIR" cargo test export_bindings )

# ts-rs maps Rust 64-bit ints (u64/i64) to `bigint`. Every such field in this API
# is a millisecond timestamp, byte count, count, or external id all safely
# within Number.MAX_SAFE_INTEGER and the whole client treats them as `number`.
# Normalize them back so client arithmetic keeps working. (Portable sed.)
for f in "$GEN_DIR"/*.ts; do
  sed -i.bak 's/bigint/number/g' "$f" && rm -f "$f.bak"
done

# Barrel: re-export every generated type so `@luma/core` can `export * from './generated'`.
{
  echo "// Auto-generated barrel for the ts-rs bindings. Do not edit run scripts/gen-types.sh."
  for f in "$GEN_DIR"/*.ts; do
    name="$(basename "$f" .ts)"
    [ "$name" = "index" ] && continue
    echo "export type { $name } from './$name';"
  done
} > "$GEN_DIR/index.ts"

count="$(find "$GEN_DIR" -name '*.ts' ! -name 'index.ts' | wc -l | tr -d ' ')"
echo "✓ Generated $count TS wire types → $GEN_DIR"
