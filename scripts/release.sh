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

# A run whose job executed zero steps means GitHub never started it. An
# account-level billing lock looks exactly like this, and a tag pushed under
# it creates no release: releases/latest keeps resolving to the previous one
# and install.sh silently serves the old version.
has_gh=0
if command -v gh >/dev/null 2>&1; then
  has_gh=1
fi

run_steps() {
  gh run view "$1" --json jobs --jq '.jobs[0].steps | length' 2>/dev/null || echo ""
}

newest_run_id() {
  gh run list --limit 1 --json databaseId --jq '.[0].databaseId' 2>/dev/null || echo ""
}

newest_release_run_id() {
  gh run list --workflow=release.yml --limit 1 --json databaseId --jq '.[0].databaseId' 2>/dev/null || echo ""
}

if [ "$has_gh" -eq 1 ] && [ "${NOBODY_RELEASE_FORCE:-0}" != "1" ]; then
  probe="$(newest_run_id)"
  if [ -n "$probe" ] && [ "$(run_steps "$probe")" = "0" ]; then
    echo "error: the newest workflow run executed no steps, so Actions is not" >&2
    echo "       starting jobs on this account. Tagging now would create no" >&2
    echo "       release. Fix billing at https://github.com/settings/billing," >&2
    echo "       or override with NOBODY_RELEASE_FORCE=1." >&2
    exit 1
  fi
fi

sed -i "s/^version = \".*\"/version = \"${version}\"/" Cargo.toml
cargo update --workspace --offline >/dev/null

git add Cargo.toml Cargo.lock
if git diff --cached --quiet; then
  echo "version already ${version}"
else
  git commit -m "release ${tag}"
fi

before=""
if [ "$has_gh" -eq 1 ]; then
  before="$(newest_release_run_id)"
fi

git tag -a "$tag" -m "nobody ${tag}"
git push origin HEAD
git push origin "$tag"

if [ "$has_gh" -eq 0 ]; then
  echo
  echo "tag ${tag} pushed. gh not found, so it was not verified."
  echo "check https://github.com/nuevosik/Nobody/actions before announcing it."
  exit 0
fi

if [ "${NOBODY_RELEASE_WAIT:-1}" != "1" ]; then
  echo
  echo "tag ${tag} pushed. NOBODY_RELEASE_WAIT=0, so it was not verified."
  exit 0
fi

echo
echo "tag ${tag} pushed; waiting for the release workflow to start."

seen=""
for _ in $(seq 1 12); do
  current="$(newest_release_run_id)"
  if [ -n "$current" ] && [ "$current" != "$before" ]; then
    seen="$current"
    break
  fi
  sleep 10
done

if [ -z "$seen" ]; then
  echo "error: no release workflow run appeared within 2 minutes of the push." >&2
  echo "       Actions is not starting jobs, so no release was created and" >&2
  echo "       install.sh still serves the previous version." >&2
  echo "       re-run with NOBODY_RELEASE_FORCE=1 NOBODY_RELEASE_WAIT=0 after" >&2
  echo "       fixing billing, or delete the tag:  git push origin :${tag}" >&2
  exit 1
fi

if [ "$(run_steps "$seen")" = "0" ]; then
  echo "error: run ${seen} executed no steps, so Actions is not starting jobs." >&2
  echo "       no release was created for ${tag}." >&2
  exit 1
fi

echo "run ${seen} started; waiting for it to finish."
minutes="${NOBODY_RELEASE_WAIT_MIN:-60}"
waited=0
while :; do
  status="$(gh run view "$seen" --json status --jq '.status' 2>/dev/null || echo unknown)"
  if [ "$status" = "completed" ]; then
    break
  fi
  if [ "$waited" -ge $(( minutes * 6 )) ]; then
    echo "error: run ${seen} is still '${status}' after ${minutes} minutes." >&2
    echo "       inspect it:  gh run view ${seen}" >&2
    exit 1
  fi
  sleep 10
  waited=$(( waited + 1 ))
  if [ $(( waited % 6 )) -eq 0 ]; then
    echo "  still '${status}' after $(( waited / 6 )) min"
  fi
done

if gh release view "$tag" >/dev/null 2>&1; then
  echo "release ${tag} published."
  echo
  echo "install it with:"
  echo "  curl -fsSL https://github.com/nuevosik/Nobody/releases/latest/download/install.sh | sh"
  exit 0
fi

conclusion="$(gh run view "$seen" --json conclusion --jq '.conclusion' 2>/dev/null || echo unknown)"
echo "error: run ${seen} finished as '${conclusion}' and no release was published." >&2
echo "       releases/latest still resolves to the previous release, so install.sh" >&2
echo "       would silently install the old version." >&2
echo "       inspect:  gh run view ${seen} --log-failed" >&2
exit 1
