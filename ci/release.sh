#!/usr/bin/env bash
#
# `release` in the devenv shell. The order below is the whole point: every guard
# runs before anything is created, so a rejected release leaves no tag, no image,
# and nothing to undo.
#
# The tag is pushed from here rather than from a workflow because a push
# carrying GITHUB_TOKEN starts no workflow run, and this one has to start
# release.yml. docs/adr/0053-guarded-releases-from-a-local-command.md.
set -euo pipefail

cd "${DEVENV_ROOT:-$(git rev-parse --show-toplevel)}"

readonly guards=ci/release-guards.sh
readonly watch_timeout_seconds=1800
readonly requested=${1:-patch}

refuse() {
  printf 'refusing to release: %s\n' "$*" >&2
  exit 1
}

command -v gh >/dev/null 2>&1 \
  || refuse 'gh is missing, and gh is what waits for the release run -- enter the devenv shell, or: nix profile install nixpkgs#gh'
gh auth status >/dev/null 2>&1 \
  || refuse 'gh is not authenticated, and gh is what waits for the release run -- run: gh auth login'

branch=$(git symbolic-ref --quiet --short HEAD) || refuse 'HEAD is detached -- release from main'
[[ $branch == main ]] || refuse "on branch $branch -- release from main"
[[ -z $(git status --porcelain) ]] || refuse 'the working tree is dirty -- commit or stash first'

git fetch --quiet origin main
git merge-base --is-ancestor origin/main HEAD \
  || refuse 'main is behind origin/main -- git pull first'

case $requested in
  major | minor | patch) version=$(git tag --list 'v*' | "$guards" next "$requested") ;;
  *) version=$requested ;;
esac

commit=$(git rev-parse HEAD)
"$guards" assert-releasable "$version"
"$guards" assert-tag-unused "$version"
"$guards" assert-on-main "$commit"
"$guards" assert-unpublished "$version"

if "$guards" is-stable "$version"; then
  latest="moves to $version"
else
  latest="unchanged -- $version is a prerelease"
fi

printf '\n'
printf '  commit   %s  %s\n' "$(git rev-parse --short HEAD)" "$(git log -1 --format=%s)"
printf '  version  %s\n' "$version"
printf '  image    %s:%s\n' "$("$guards" image)" "${version#v}"
printf '  :latest  %s\n' "$latest"
printf '\n'

read -rp 'Release this? Type Y to continue: ' confirmation
[[ $confirmation == Y ]] || refuse 'not confirmed'

# `warm-cache` is the gate plus the push, and it asserts the push actually
# landed -- cachix marks a rejected push with a red x and still exits 0. The
# image is in `checks` now, so the gate covers it and there is nothing to build
# here beyond it.
printf '\nwarming the cache, so the release run is a hit...\n'
warm-cache

git tag -a "$version" -m "Release $version" "$commit"
git push origin "refs/tags/$version"

printf '\nwaiting for the release run...\n'
run_id=''
deadline=$((SECONDS + watch_timeout_seconds))
while ((SECONDS < deadline)); do
  # NOTE: polled for this release's run rather than taken as the newest, which
  # would land on the wrong one — the push and the run appearing are seconds
  # apart. Matched on the commit as well as the tag: release.yml only ever runs
  # on a tag push, so the sha identifies the run, and `head_branch` holding a tag
  # name is undocumented behaviour to lean on alone.
  run_id=$(gh run list --workflow release.yml --limit 20 \
    --json databaseId,headBranch,headSha \
    --jq "map(select(.headBranch == \"$version\" or .headSha == \"$commit\")) | first | .databaseId // empty") || true
  if [[ -n $run_id ]]; then break; fi
  sleep 5
done

if [[ -z $run_id ]]; then
  printf 'no release run appeared for %s within %ds. The tag is pushed -- look for the run with:\n  gh run list --workflow release.yml\n' \
    "$version" "$watch_timeout_seconds" >&2
  exit 1
fi

run_url=$(gh run view "$run_id" --json url --jq .url)
printf '  %s\n\n' "$run_url"

remaining=$((deadline - SECONDS))
# NOTE: never 0 -- `timeout 0` is `timeout never`, and never is the one outcome
# this wait must not have.
((remaining > 0)) || remaining=60

watch_status=0
timeout "$remaining" gh run watch "$run_id" --exit-status --interval 10 || watch_status=$?

if ((watch_status == 0)); then
  printf '\nreleased %s\n' "$version"
  printf '  image  %s:%s\n' "$("$guards" image)" "${version#v}"
  printf '  run    %s\n' "$run_url"
  exit 0
fi

if ((watch_status == 124)); then
  printf '\nstill running after %ds -- not a failure, just longer than the wait:\n  %s\n' \
    "$watch_timeout_seconds" "$run_url" >&2
  exit 1
fi

{
  printf '\nTHE RELEASE RUN FAILED. Nothing was published.\n'
  printf '  run  %s\n' "$run_url"
  printf '\n%s is now taken, and the guards will refuse it again. Fix main and release the\n' "$version"
  printf 'next version -- or retire this tag yourself with:\n  git push origin --delete %s\n' "$version"
} >&2
exit 1
