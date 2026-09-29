#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

version="${1:-}"
if [[ -z "$version" ]]; then
  echo "usage: $0 <version>   e.g. $0 0.1.1" >&2
  exit 1
fi
version="${version#v}"
tag="v${version}"

if [[ -n "$(git status --porcelain)" ]]; then
  echo "working tree is dirty; commit or stash first" >&2
  exit 1
fi

if git rev-parse "$tag" >/dev/null 2>&1; then
  echo "tag $tag already exists" >&2
  exit 1
fi

sed -i "s/^version = \".*\"/version = \"${version}\"/" Cargo.toml
cargo update --workspace --offline >/dev/null

git add Cargo.toml Cargo.lock
if git diff --cached --quiet; then
  echo "version already ${version}"
else
  git commit -m "release ${tag}"
fi

git tag -a "$tag" -m "nobody ${tag}"
git push origin HEAD
git push origin "$tag"

echo
echo "tag ${tag} pushed; waiting for the release to be published."

if command -v gh >/dev/null 2>&1; then
  published=0
  for _ in {1..90}; do
    if gh release view "$tag" >/dev/null 2>&1; then
      published=1
      break
    fi
    sleep 10
  done
  if [ "$published" -ne 1 ]; then
    echo "error: ${tag} was tagged and pushed but no release was published." >&2
    echo "       releases/latest still resolves to the previous release, so install.sh" >&2
    echo "       would silently install the old version." >&2
    echo "       check the run:  gh run list --workflow=release.yml --limit 3" >&2
    exit 1
  fi
  echo "release ${tag} published."
  echo
  echo "install it with:"
  echo "  curl -fsSL https://github.com/nuevosik/Nobody/releases/latest/download/install.sh | sh"
else
  echo "gh not found; could not confirm that ${tag} was published." >&2
  echo "verify the Actions tab before announcing the release:" >&2
  echo "  https://github.com/nuevosik/Nobody/actions" >&2
fi
