#!/usr/bin/env bash
set -euo pipefail
# ponytail: no systemd unit yet; autostart `nobody &` from your compositor, add a unit when you need session integration
PREFIX="${PREFIX:-$HOME/.local}"

cargo build --release
install -Dm755 target/release/nobody "$PREFIX/bin/nobody"
echo "installed to $PREFIX/bin/nobody"
