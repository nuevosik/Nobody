#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
! rg -n 'infrastructure::|crate::(daemon|icons)' src/presentation
! rg -n 'presentation::' src/infrastructure
cargo test --all-targets

echo "all checks passed"
