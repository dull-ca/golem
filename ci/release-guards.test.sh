#!/usr/bin/env bash
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

expect_stdout ghcr.io/dull-ca/golem-docs 'the guard and the publish name one image' '' "$guards" image

if [[ $failures -ne 0 ]]; then
  printf '%d guard test(s) failed\n' "$failures"
  exit 1
fi

printf 'release guards hold\n'
