#!/usr/bin/env bash
#
# The only copy of what makes a release legal. Both callers — ci/release.sh and
# .github/workflows/release.yml — run this file rather than their own version of
# it, the image name included, so the reference a guard inspects is the one the
# publish writes. docs/adr/0053-guarded-releases-from-a-local-command.md.
set -euo pipefail

readonly published_image=ghcr.io/dull-ca/golem-docs
readonly stable_pattern='^v[0-9]+\.[0-9]+\.[0-9]+$'
readonly releasable_pattern='^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$'

refuse() {
  if [[ ${GITHUB_ACTIONS-} == true ]]; then
    printf '::error::%s\n' "$*" >&2
  else
    printf 'refusing to release: %s\n' "$*" >&2
  fi
  return 1
}

assert_releasable() {
  local version=${1-}
  [[ $version =~ $releasable_pattern ]] || refuse \
    "$(printf '%q is not a release version -- expected vMAJOR.MINOR.PATCH (v1.2.3) or a prerelease of one (v1.2.3-rc1)' "$version")"
}

is_stable() {
  [[ ${1-} =~ $stable_pattern ]]
}

latest_stable() {
  local latest
  latest=$(grep -E "$stable_pattern" | sed 's/^v//' | sort -t. -k1,1n -k2,2n -k3,3n | tail -1) || true
  printf '%s' "${latest:-0.0.0}"
}

next() {
  local bump=${1-} base major minor patch
  base=$(latest_stable)
  IFS=. read -r major minor patch <<<"$base"
  case $bump in
    major) printf 'v%d.0.0\n' "$((major + 1))" ;;
    minor) printf 'v%d.%d.0\n' "$major" "$((minor + 1))" ;;
    patch) printf 'v%d.%d.%d\n' "$major" "$minor" "$((patch + 1))" ;;
    *) refuse "$(printf '%q is not a bump -- expected major, minor, or patch' "$bump")" ;;
  esac
}

assert_tag_unused() {
  local version=${1-}
  git rev-parse -q --verify "refs/tags/$version" >/dev/null || return 0
  refuse "$version already exists, at $(git rev-list -n1 "$version") -- a released version is never re-pointed; release the next version instead"
}

assert_on_main() {
  local commit=${1-}
  # NOTE: fetched here, not assumed. A runner checked out at a tag has the
  # commits but no origin/main ref to compare against.
  git fetch --quiet --no-tags origin main
  git merge-base --is-ancestor "$commit" FETCH_HEAD && return 0
  refuse "$commit is not an ancestor of origin/main -- only what is merged to main is releasable; merge it first, then release the merge result"
}

assert_unpublished() {
  local version=${1-} reference="docker://$published_image:${1#v}"
  local -a skopeo=(skopeo) authfile=()
  command -v skopeo >/dev/null 2>&1 || skopeo=(nix run nixpkgs#skopeo --)
  if [[ -n ${GHCR_AUTHFILE-} ]]; then authfile=(--authfile "$GHCR_AUTHFILE"); fi
  "${skopeo[@]}" inspect --no-tags "${authfile[@]}" "$reference" >/dev/null 2>&1 || return 0
  refuse "$published_image:${version#v} is already published -- one version string names one artifact forever; release the next version instead"
}

case ${1-} in
  image) printf '%s\n' "$published_image" ;;
  assert-releasable) assert_releasable "${2-}" ;;
  is-stable) is_stable "${2-}" ;;
  next) next "${2-}" ;;
  assert-tag-unused) assert_tag_unused "${2-}" ;;
  assert-on-main) assert_on_main "${2-}" ;;
  assert-unpublished) assert_unpublished "${2-}" ;;
  *)
    printf 'usage: release-guards.sh {image|assert-releasable|is-stable|next|assert-tag-unused|assert-on-main|assert-unpublished} ARGUMENT\n' >&2
    exit 2
    ;;
esac
