#!/usr/bin/env bash
# Cut a release: bump version, tag, push. GitHub Actions builds the artifact.
#   ./scripts/release.sh 0.1.1
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
cargo generate-lockfile --offline >/dev/null 2>&1 || cargo generate-lockfile >/dev/null

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
echo "tag ${tag} pushed. Actions will attach ${tag} artifacts."
echo "  https://github.com/nuevosik/Nobody/actions"
echo
echo "install from the release once it finishes:"
echo "  curl -fsSL https://github.com/nuevosik/Nobody/releases/latest/download/install.sh | sh"
