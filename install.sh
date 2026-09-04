#!/bin/sh
# Install nobody from the latest GitHub Release.
#   curl -fsSL https://github.com/nuevosik/Nobody/releases/latest/download/install.sh | sh
set -eu

REPO="nuevosik/Nobody"
NAME="nobody"
DEST="${NOBODY_INSTALL_DIR:-${HOME}/.local/bin}"

die() {
  printf 'nobody install: %s\n' "$*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing $1"
}

os=$(uname -s)
arch=$(uname -m)
case "$os" in
  Linux) os="linux" ;;
  *) die "unsupported OS: $os (Linux only for now)" ;;
esac
case "$arch" in
  x86_64 | amd64) arch="x86_64" ;;
  aarch64 | arm64) arch="aarch64" ;;
  *) die "unsupported arch: $arch" ;;
esac

asset="${NAME}-${arch}-unknown-${os}-gnu.tar.gz"
url="https://github.com/${REPO}/releases/latest/download/${asset}"

need curl
need tar
need install

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT HUP

printf 'downloading %s\n' "$url"
curl -fsSL --retry 3 -o "$tmp/${asset}" "$url" || die "download failed (is there a release?)"
tar -C "$tmp" -xzf "$tmp/${asset}"
[ -f "$tmp/${NAME}" ] || die "archive did not contain ${NAME}"

mkdir -p "$DEST"
install -m 0755 "$tmp/${NAME}" "$DEST/${NAME}"

printf 'installed %s\n' "$DEST/${NAME}"
printf '\nAutostart it from your compositor, e.g.:\n'
printf '  nobody &\n'
printf '\nMake sure %s is on PATH, then run:  %s\n' "$DEST" "$NAME"
