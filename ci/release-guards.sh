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
readonly conventional_header_pattern='^[a-zA-Z][a-zA-Z0-9]*(\([^)]*\))?(!)?: .'
readonly breaking_footer_pattern='^BREAKING[ -]CHANGE[[:space:]]*:'

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
  printf '%s\n' "${latest:-0.0.0}"
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

# Every conventional type is worth at least a patch, not just `feat` and `fix`.
# What a release publishes is the docs image, so a range of nothing but `docs:`
# is a range with something to ship, and a range of nothing but `chore:` still
# moves the site the image serves.
#
# A range with no conventional commit in it at all reads `none` rather than
# `patch`, which is a different answer from "the smallest bump": it lets the
# caller refuse instead of releasing a version nobody chose.
conventional_bump() {
  local message='' header type bump='none' unread_records=1
  while ((unread_records)); do
    IFS= read -r -d '' message || unread_records=0
    header=$(printf '%s\n' "$message" | grep -m1 -v '^[[:space:]]*$' || true)
    if [[ $header =~ $conventional_header_pattern ]]; then
      if [[ ${BASH_REMATCH[2]} == '!' ]] \
        || printf '%s\n' "$message" | grep -qE "$breaking_footer_pattern"; then
        printf 'major\n'
        return 0
      fi
      type=${header%%[(!:]*}
      if [[ ${type,,} == feat ]]; then
        bump='minor'
      elif [[ $bump == none ]]; then
        bump='patch'
      fi
    fi
    message=''
  done
  printf '%s\n' "$bump"
}

# Below 1.0 a breaking change is a minor bump, because under Cargo's
# compatibility rules a 0.x minor bump already *is* the breaking bump: `0.3` and
# `0.4` are incompatible ranges, so `major` has nothing left to express that
# `minor` does not. Letting a `!` reach `v1.0.0` would spend the one version
# string that is a claim about stability on what is only a description of a
# change. `release major`, typed deliberately, is the sole path to `v1.0.0`.
effective_bump() {
  local bump=${1-} base=${2-}
  case $bump in
    major | minor | patch) ;;
    *) refuse "$(printf '%q is not a bump -- expected major, minor, or patch' "$bump")" ;;
  esac
  [[ $base =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] \
    || refuse "$(printf '%q is not a released version -- expected MAJOR.MINOR.PATCH' "$base")"
  if [[ $bump == major && $base == 0.* ]]; then
    printf 'minor\n'
  else
    printf '%s\n' "$bump"
  fi
}

workspace_version() {
  awk '
    /^[[:space:]]*\[/ { in_workspace_package = ($0 ~ /^[[:space:]]*\[workspace\.package\][[:space:]]*$/) }
    in_workspace_package && !found && /^[[:space:]]*version[[:space:]]*=/ && match($0, /"[^"]*"/) {
      print substr($0, RSTART + 1, RLENGTH - 2)
      found = 1
    }
    END { if (!found) exit 1 }
  ' || refuse 'Cargo.toml has no version under [workspace.package] -- the release cannot read a crate version'
}

# NOTE: `[workspace.package]` is the only version written, and every crate that
# ships reads it with `version.workspace = true`. `libs/workspace-hack` is the
# one that does not, on purpose: it is `publish = false`, it exists only to
# unify feature flags, and `cargo hakari generate` rewrites the file wholesale —
# a version set here would be reverted the next time the hack is regenerated.
set_workspace_version() {
  local version=${1-}
  [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$ ]] \
    || refuse "$(printf '%q is not a crate version -- expected MAJOR.MINOR.PATCH, without the v' "$version")"
  awk -v version="$version" '
    /^[[:space:]]*\[/ { in_workspace_package = ($0 ~ /^[[:space:]]*\[workspace\.package\][[:space:]]*$/) }
    in_workspace_package && !replaced && /^[[:space:]]*version[[:space:]]*=/ {
      sub(/=.*/, "= \"" version "\"")
      replaced = 1
    }
    { print }
    END { if (!replaced) exit 1 }
  ' || refuse 'Cargo.toml has no version under [workspace.package] -- the release cannot set a crate version'
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
  latest-stable) latest_stable ;;
  next) next "${2-}" ;;
  conventional-bump) conventional_bump ;;
  effective-bump) effective_bump "${2-}" "${3-}" ;;
  workspace-version) workspace_version ;;
  set-workspace-version) set_workspace_version "${2-}" ;;
  assert-tag-unused) assert_tag_unused "${2-}" ;;
  assert-on-main) assert_on_main "${2-}" ;;
  assert-unpublished) assert_unpublished "${2-}" ;;
  *)
    printf 'usage: release-guards.sh {image|assert-releasable|is-stable|latest-stable|next|conventional-bump|effective-bump|workspace-version|set-workspace-version|assert-tag-unused|assert-on-main|assert-unpublished} ARGUMENT\n' >&2
    exit 2
    ;;
esac
