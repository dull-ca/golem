#!/usr/bin/env bash
#
# golem's half of the release. The sequence, the guards, and the version
# derivation are dull-nix's `mkReleaseCommand`; what a release *of golem*
# publishes and which file carries its version are only knowable here.
# docs/adr/0056-the-release-command-is-a-shared-flake-input.md.
#
# All four of `assert-ready`, `describe`, `assert-unpublished` and `set-version`
# are answered, and a repository supplying one must supply all four. A hook the
# driver calls and this file does not implement falls through to the usage arm,
# and a publish guard that silently does not run is worse than one that was
# never written.
#
# `image` is a fifth subcommand and is no part of that contract -- `release`
# never calls it. `.github/workflows/release.yml` does, along with
# `assert-unpublished`, running this file straight out of the checkout rather
# than through the wrapper. That is the point: one file holds the image name, so
# the reference the guard inspects is the reference the publish writes
# (ADR 0053).
#
# Two callers, two ways `release-guards` gets on PATH: the wrapper puts it there
# for `release`, and release.yml has a step of its own. Only `describe` and
# `set-version` need it.
set -euo pipefail

readonly published_image=ghcr.io/dull-ca/golem-docs

refuse() {
  if [[ ${GITHUB_ACTIONS-} == true ]]; then
    printf '::error::%s\n' "$*" >&2
  else
    printf 'refusing to release: %s\n' "$*" >&2
  fi
  return 1
}

assert_ready() {
  command -v cargo >/dev/null 2>&1 \
    || refuse 'cargo is missing, and cargo is what relocks the crate version -- enter the devenv shell'
}

# `nix run` is the fallback rather than the requirement, so this hook works both
# in the devenv shell (skopeo on PATH) and in release.yml (nothing but nix).
#
# NOTE: a failed `inspect` is read as "not published", which is what it means
# for a tag that does not exist -- and also what it looks like when the registry
# is unreachable or the credentials are wrong. The guard is therefore only as
# strong as the connection; it catches the mistake it was written for (releasing
# a version already published) and does not claim to be a proof of absence.
assert_unpublished() {
  local version=${1-} reference="docker://$published_image:${1#v}"
  local -a skopeo=(skopeo) authfile=()
  command -v skopeo >/dev/null 2>&1 || skopeo=(nix run nixpkgs#skopeo --)
  if [[ -n ${GHCR_AUTHFILE-} ]]; then authfile=(--authfile "$GHCR_AUTHFILE"); fi
  "${skopeo[@]}" inspect --no-tags "${authfile[@]}" "$reference" >/dev/null 2>&1 || return 0
  refuse "$published_image:${version#v} is already published -- one version string names one artifact forever; release the next version instead"
}

# Printed above the confirmation and again on success. `%-9s` is not decoration:
# `release` prints its own `commit` and `version` rows at that width, and these
# lines sit under them.
describe() {
  local version=${1-} latest
  if release-guards is-stable "$version"; then
    latest="moves to $version"
  else
    latest="unchanged -- $version is a prerelease"
  fi
  printf '%-9s %s -> %s\n' crate \
    "$(release-guards cargo-workspace-version <Cargo.toml)" "${version#v}"
  printf '%-9s %s:%s\n' image "$published_image" "${version#v}"
  printf '%-9s %s\n' ':latest' "$latest"
}

# The two paths printed at the end are the contract: `release` stages what
# `set-version` says it wrote, and names no version-bearing file itself. Adding
# a third file to golem's release commit is a third `printf` here and no change
# to dull-nix.
#
# NOTE: `[workspace.package]` is the only version written, and every crate that
# ships reads it with `version.workspace = true`. `libs/workspace-hack` is the
# one that does not, on purpose: it is `publish = false`, it exists only to
# unify feature flags, and `cargo hakari generate` rewrites the file wholesale —
# a version set here would be reverted the next time the hack is regenerated.
#
# The rewrite goes through a temporary file because a redirect into `Cargo.toml`
# would truncate it before awk had read it. A failed rewrite therefore leaves
# `Cargo.toml` as it was; a failed `cargo update` does not, and does not need
# to -- `release` unwinds a failed `set-version` with `git reset --hard` to the
# commit it showed you.
set_version() {
  local version=${1-} rewritten status=0
  rewritten=$(mktemp)
  release-guards set-cargo-workspace-version "$version" <Cargo.toml >"$rewritten" \
    && cat "$rewritten" >Cargo.toml \
    && cargo update --workspace --offline --quiet \
    || status=1
  rm -f "$rewritten"
  ((status == 0)) || return 1
  printf 'Cargo.toml\nCargo.lock\n'
}

case ${1-} in
  assert-ready) assert_ready ;;
  assert-unpublished) assert_unpublished "${2-}" ;;
  describe) describe "${2-}" ;;
  set-version) set_version "${2-}" ;;
  image) printf '%s\n' "$published_image" ;;
  *)
    printf 'usage: release-hooks {assert-ready|assert-unpublished|describe|set-version|image} ARGUMENT\n' >&2
    exit 2
    ;;
esac
