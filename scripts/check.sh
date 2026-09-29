#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

command -v cargo >/dev/null || { echo "cargo not found; nothing to check" >&2; exit 1; }
command -v rg >/dev/null || {
  echo "rg not found; the layer guards would silently pass" >&2
  exit 1
}

cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
! rg -n 'infrastructure::|crate::(daemon|icons)' src/presentation
! rg -n 'presentation::' src/infrastructure
cargo test --all-targets

echo "all checks passed"
