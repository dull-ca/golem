#!/usr/bin/env bash
#
# The guards that need neither a repository nor a network, exercised through the
# same subcommand dispatch both callers use. The guards path is an argument so
# the copy under test can be named: `nix flake check` builds this in a sandbox
# holding a shebang-patched `ci/`, not the checkout.
# docs/adr/0053-guarded-releases-from-a-local-command.md.
#
# `conventional-bump` reads NUL-separated records, because a commit message
# contains blank lines and `git log --format='%B%x00'` is what feeds it. Hence
# `expect_stdout_of_escaped`, which is `expect_stdout` with `printf %b` so a
# case can write `\0` between messages and `\n` inside one.
set -uo pipefail

guards=${1:?usage: release-guards.test.sh <path to release-guards.sh>}

failures=0

expect_exit() {
  local expected=$1 description=$2
  shift 2
  local output
  output=$("$@" 2>&1)
  local actual=$?
  if [[ $actual -ne $expected ]]; then
    printf 'FAIL %s: expected exit %d, got %d (%s)\n' "$description" "$expected" "$actual" "$output"
    failures=$((failures + 1))
  fi
}

expect_stdout() {
  local expected=$1 description=$2 stdin=$3
  shift 3
  local actual
  actual=$(printf '%s' "$stdin" | "$@")
  if [[ $actual != "$expected" ]]; then
    printf 'FAIL %s: expected %s, got %s\n' "$description" "$expected" "$actual"
    failures=$((failures + 1))
  fi
}

expect_stdout_of_escaped() {
  local expected=$1 description=$2 stdin=$3
  shift 3
  local actual
  actual=$(printf '%b' "$stdin" | "$@")
  if [[ $actual != "$expected" ]]; then
    printf 'FAIL %s: expected %s, got %s\n' "$description" "$expected" "$actual"
    failures=$((failures + 1))
  fi
}

expect_exit 0 'accepts a stable version' "$guards" assert-releasable v1.2.3
expect_exit 0 'accepts a hyphenated prerelease' "$guards" assert-releasable v1.2.3-rc1
expect_exit 0 'accepts a dotted prerelease' "$guards" assert-releasable v0.4.0-rc.1
expect_exit 1 'rejects a missing v prefix' "$guards" assert-releasable 1.2.3
expect_exit 1 'rejects a two-component version' "$guards" assert-releasable v1.2
expect_exit 1 'rejects a non-numeric version' "$guards" assert-releasable vfoo
expect_exit 1 'rejects a four-component version' "$guards" assert-releasable v1.2.3.4
expect_exit 1 'rejects an empty version' "$guards" assert-releasable ''
expect_exit 1 'rejects a leading-space version' "$guards" assert-releasable ' v1.2.3'

expect_exit 0 'v1.2.3 is stable' "$guards" is-stable v1.2.3
expect_exit 1 'v1.2.3-rc1 is not stable' "$guards" is-stable v1.2.3-rc1
expect_exit 1 'v0.4.0-rc.1 is not stable' "$guards" is-stable v0.4.0-rc.1

existing_tags='v0.1.0
v0.2.0
v0.3.0'

expect_stdout v0.3.1 'patch bumps the latest stable tag' "$existing_tags" "$guards" next patch
expect_stdout v0.4.0 'minor bumps the latest stable tag' "$existing_tags" "$guards" next minor
expect_stdout v1.0.0 'major bumps the latest stable tag' "$existing_tags" "$guards" next major

expect_stdout v0.11.0 'ordering is numeric, not lexical' 'v0.9.0
v0.10.0' "$guards" next minor

expect_stdout v0.4.0 'prereleases do not become the bump base' 'v0.3.0
v0.4.0-rc1' "$guards" next minor

expect_stdout v0.0.1 'the first release bumps from nothing' '' "$guards" next patch

expect_stdout v0.3.1 'a tag that merely contains a version is not one' 'v0.3.0
golem-v9.9.9' "$guards" next patch

expect_exit 1 'rejects an unknown bump' "$guards" next sideways

expect_stdout 0.3.1 'the latest stable tag is the release base' "$existing_tags
v0.3.1" "$guards" latest-stable
expect_stdout 0.0.0 'no tags means no base' '' "$guards" latest-stable

expect_stdout_of_escaped minor 'feat asks for a minor' \
  'feat: a thing\0' "$guards" conventional-bump
expect_stdout_of_escaped patch 'fix asks for a patch' \
  'fix: a thing\0' "$guards" conventional-bump
expect_stdout_of_escaped patch 'docs ships the docs image, so it asks for a patch' \
  'docs(website): a page\0' "$guards" conventional-bump
expect_stdout_of_escaped patch 'chore, ci, style, test and refactor all ask for a patch' \
  'chore: a\0ci: b\0style: c\0test: d\0refactor: e\0' "$guards" conventional-bump
expect_stdout_of_escaped minor 'the loudest commit wins, whatever its position' \
  'fix: a\0feat: b\0docs: c\0' "$guards" conventional-bump
expect_stdout_of_escaped major 'a bang in the header is breaking' \
  'fix: a\0feat(emet)!: b\0' "$guards" conventional-bump
expect_stdout_of_escaped major 'a BREAKING CHANGE footer is breaking' \
  'fix: a\0refactor: b\n\nBREAKING CHANGE: the wire format moved\0' "$guards" conventional-bump
expect_stdout_of_escaped major 'the hyphenated footer spelling counts too' \
  'refactor: b\n\nBREAKING-CHANGE: the wire format moved\0' "$guards" conventional-bump
expect_stdout_of_escaped none 'nothing since the last tag asks for nothing' \
  '' "$guards" conventional-bump
expect_stdout_of_escaped none 'unconventional commits ask for nothing' \
  'Initial commit\0merge branch main\0' "$guards" conventional-bump
expect_stdout_of_escaped patch 'unconventional commits do not silence conventional ones' \
  'Initial commit\0fix: a\0' "$guards" conventional-bump
expect_stdout_of_escaped patch 'a trailing record without its separator still counts' \
  'fix: a' "$guards" conventional-bump
expect_stdout_of_escaped patch 'the leading newline git writes between records is not a header' \
  'fix: a\0\nfix: b\0' "$guards" conventional-bump
expect_stdout_of_escaped none 'a bare type with no summary is not conventional' \
  'feat:\0feat\0' "$guards" conventional-bump
expect_stdout_of_escaped patch 'a mid-line mention of the footer is not the footer' \
  'docs: describe BREAKING CHANGE: notation\0' "$guards" conventional-bump
expect_stdout_of_escaped minor 'a type is recognised whatever its casing' \
  'Feat(emet): a thing\0' "$guards" conventional-bump
expect_stdout_of_escaped patch 'feature is not the conventional spelling of feat' \
  'feature: a thing\0' "$guards" conventional-bump

expect_stdout minor 'pre-1.0, a breaking change is a minor bump' '' \
  "$guards" effective-bump major 0.3.1
expect_stdout minor 'pre-1.0, a feature is still a minor bump' '' \
  "$guards" effective-bump minor 0.3.1
expect_stdout patch 'pre-1.0, a fix is still a patch bump' '' \
  "$guards" effective-bump patch 0.3.1
expect_stdout major 'from 1.0 on, a breaking change is a major bump' '' \
  "$guards" effective-bump major 1.4.2
expect_stdout minor 'from 1.0 on, a feature is a minor bump' '' \
  "$guards" effective-bump minor 1.4.2
expect_stdout major 'a 10.x base is not a 0.x base' '' \
  "$guards" effective-bump major 10.0.0
expect_exit 1 'refuses to soften a bump it does not recognise' \
  "$guards" effective-bump none 0.3.1
expect_exit 1 'refuses a base that is not a version' \
  "$guards" effective-bump major v0.3.1

cargo_toml='[workspace]
resolver = "2"

[workspace.package]
version     = "0.1.0"
edition     = "2021"

[workspace.dependencies]
serde        = { version = "1", features = ["derive"] }
'

expect_stdout '[workspace]
resolver = "2"

[workspace.package]
version     = "0.3.2"
edition     = "2021"

[workspace.dependencies]
serde        = { version = "1", features = ["derive"] }' \
  'the crate version follows the tag, and only under [workspace.package]' \
  "$cargo_toml" "$guards" set-workspace-version 0.3.2

expect_stdout 0.1.0 'the crate version is read from [workspace.package], not from a dependency' \
  "$cargo_toml" "$guards" workspace-version
expect_exit 1 'refuses a Cargo.toml with no workspace version to read' \
  bash -c "printf '%s' '[workspace]
resolver = \"2\"

[workspace.dependencies]
serde = { version = \"1\" }
' | $guards workspace-version"

expect_exit 1 'refuses a Cargo.toml with no workspace version to set' \
  bash -c "printf '%s' '[workspace]
resolver = \"2\"
' | $guards set-workspace-version 0.3.2"
expect_exit 1 'refuses a crate version carrying the v prefix' \
  bash -c "printf '%s' '$cargo_toml' | $guards set-workspace-version v0.3.2"

expect_stdout ghcr.io/dull-ca/golem-docs 'the guard and the publish name one image' '' "$guards" image

if [[ $failures -ne 0 ]]; then
  printf '%d guard test(s) failed\n' "$failures"
  exit 1
fi

printf 'release guards hold\n'
