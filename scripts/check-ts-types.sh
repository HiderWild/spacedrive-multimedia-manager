#!/usr/bin/env bash

set -euEo pipefail

# Fail (non-zero exit) when the committed TypeScript types drift from the Rust
# `Type`-deriving structs. Regenerate into a temp copy, diff against the
# committed file, then restore the original so the working tree is never mutated.
#
# Run locally:   ./scripts/check-ts-types.sh   (or: just check-types)
# Run in CI:     same command, after the workspace is built.

_root="$(CDPATH='' cd "$(dirname "$0")/.." && pwd -P)"
_generated="${_root}/packages/ts-client/src/generated/types.ts"
_manifest="${_root}/core/Cargo.toml"

if ! command -v cargo &>/dev/null; then
  echo "cargo is required to run the type-generation drift check" >&2
  exit 1
fi

if [ ! -f "$_generated" ]; then
  echo "Generated types not found at: ${_generated}" >&2
  echo "Run the generator first: cargo run --bin generate_typescript_types --manifest-path core/Cargo.toml" >&2
  exit 1
fi

# Snapshot the committed file so the generator's in-place write can be reverted.
_backup="$(mktemp)"
cp "$_generated" "$_backup"

# Always restore the original, even on failure or interrupt, so the check never
# leaves a spurious diff in the working tree.
restore() {
  mv -f "$_backup" "$_generated"
}
trap restore EXIT

echo "Regenerating TypeScript types to check for drift..."
cargo run --quiet --bin generate_typescript_types --manifest-path "$_manifest" >/dev/null

if diff -u "$_backup" "$_generated" >/tmp/ts-types-drift.diff 2>&1; then
  echo "TypeScript types are in sync with Rust."
  exit 0
fi

echo "" >&2
echo "TypeScript types are OUT OF SYNC with Rust 'Type'-deriving structs." >&2
echo "A Rust type changed but the generated TS was not regenerated." >&2
echo "" >&2
echo "Fix it by running:" >&2
echo "  cargo run --bin generate_typescript_types --manifest-path core/Cargo.toml" >&2
echo "then commit the updated packages/ts-client/src/generated/types.ts" >&2
echo "" >&2
echo "Drift:" >&2
cat /tmp/ts-types-drift.diff >&2
exit 1
