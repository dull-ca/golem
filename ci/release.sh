#!/usr/bin/env bash
#
# `release` in the devenv shell. The order below is the whole point: every guard
# runs before anything is created, so a rejected release leaves no tag, no image,
# and nothing to undo. That holds until the release commit reaches main -- the
# one step after which a refusal leaves something behind, and by then the only
# thing left to do is the tag.
#
# The tag is pushed from here rather than from a workflow because a push
# carrying GITHUB_TOKEN starts no workflow run, and this one has to start
# release.yml. docs/adr/0053-guarded-releases-from-a-local-command.md and
# docs/adr/0055-the-version-and-changelog-come-from-the-commits.md.
set -euo pipefail

cd "${DEVENV_ROOT:-$(git rev-parse --show-toplevel)}"

readonly guards=ci/release-guards.sh
readonly cliff_config=cliff.toml
readonly changelog=CHANGELOG.md
readonly watch_timeout_seconds=1800
readonly requested=${1-}

refuse() {
  printf 'refusing to release: %s\n' "$*" >&2
  exit 1
}

command -v gh >/dev/null 2>&1 \
  || refuse 'gh is missing, and gh is what waits for the release run -- enter the devenv shell, or: nix profile install nixpkgs#gh'
gh auth status >/dev/null 2>&1 \
  || refuse 'gh is not authenticated, and gh is what waits for the release run -- run: gh auth login'
command -v git-cliff >/dev/null 2>&1 \
  || refuse 'git-cliff is missing, and git-cliff is what writes the changelog -- enter the devenv shell, or: nix profile install nixpkgs#git-cliff'
command -v cargo >/dev/null 2>&1 \
  || refuse 'cargo is missing, and cargo is what relocks the crate version -- enter the devenv shell'
[[ -f $cliff_config ]] || refuse "$cliff_config is missing, and it is the whole changelog format"

branch=$(git symbolic-ref --quiet --short HEAD) || refuse 'HEAD is detached -- release from main'
[[ $branch == main ]] || refuse "on branch $branch -- release from main"
[[ -z $(git status --porcelain) ]] || refuse 'the working tree is dirty -- commit or stash first'

git fetch --quiet origin main
git merge-base --is-ancestor origin/main HEAD \
  || refuse 'main is behind origin/main -- git pull first'

tags=$(git tag --list 'v*')
base=$(printf '%s\n' "$tags" | "$guards" latest-stable)

if git rev-parse -q --verify "refs/tags/v$base" >/dev/null; then
  released_range="v$base..HEAD"
  since="since v$base"
else
  released_range=HEAD
  since='in the whole history'
fi

unreleased_count=$(git rev-list --count "$released_range")
((unreleased_count > 0)) || refuse "nothing is unreleased $since -- there is no release to make"

if ((unreleased_count == 1)); then
  merges="1 merge $since"
else
  merges="$unreleased_count merges $since"
fi

# The version follows from the commits unless a version is named. `main` is
# squash-merged, so every commit in the range is one pull request and its
# subject is the only conventional signal that pull request left behind.
#
# A range where not one subject parses gets no version invented for it. The
# alternative is guessing a patch, and a guess here is silent: the same subjects
# that carried no bump are the ones CHANGELOG.md is about to be written from, so
# a range that says nothing about itself is one to reword. Naming the version on
# the command line is the way past this when the rewording is not worth it.
case $requested in
  '')
    conventional=$(git log --format='%B%x00' "$released_range" | "$guards" conventional-bump)
    [[ $conventional != none ]] || refuse \
      "no conventional commit is among the $merges, so no version follows from them -- reword the squash subjects, or name the version: release v${base%.*}.$((${base##*.} + 1))"
    bump=$("$guards" effective-bump "$conventional" "$base")
    version=$(printf '%s\n' "$tags" | "$guards" next "$bump")
    if [[ $bump == "$conventional" ]]; then
      derivation="$bump, read from $merges"
    else
      derivation="$bump, read from $merges -- $conventional softened, because 0.x has no compatibility to break"
    fi
    ;;
  major | minor | patch)
    version=$(printf '%s\n' "$tags" | "$guards" next "$requested")
    derivation="$requested, named on the command line"
    ;;
  *)
    version=$requested
    derivation='named on the command line'
    ;;
esac

commit=$(git rev-parse HEAD)
"$guards" assert-releasable "$version"
"$guards" assert-tag-unused "$version"
"$guards" assert-on-main "$commit"
"$guards" assert-unpublished "$version"

section=$(git-cliff --config "$cliff_config" --tag "$version" --unreleased --strip header)
[[ -n ${section//[[:space:]]/} ]] || refuse \
  "git-cliff found nothing to record for $version -- every unreleased commit is one cliff.toml skips"

if "$guards" is-stable "$version"; then
  latest="moves to $version"
else
  latest="unchanged -- $version is a prerelease"
fi

printf '\n'
printf '  commit    %s  %s\n' "$(git rev-parse --short HEAD)" "$(git log -1 --format=%s)"
printf '  version   %s  (%s)\n' "$version" "$derivation"
printf '  crate     %s -> %s\n' "$("$guards" workspace-version <Cargo.toml)" "${version#v}"
printf '  image     %s:%s\n' "$("$guards" image)" "${version#v}"
printf '  :latest   %s\n' "$latest"
printf '\n'
printf '  A release commit carrying %s and the crate version lands on main first,\n' "$changelog"
printf '  and %s tags that commit -- one past the one above.\n' "$version"
printf '\n'
# Printed before the question, not after it: this list is the last chance to
# catch a bump read from a subject that undersold its pull request. The one it
# comes from is `chore: updated docs and readme (#20)`, whose diff carried a
# systemd unit fix. Nothing downstream can see past the subject either — the
# same words become the changelog line.
printf '  Every merge the version was read from -- a squash subject is written by hand\n'
printf '  at merge time, and it is all this can see of the pull request it stands for:\n'
printf '\n'
while read -r merge; do
  asked=$(git log -1 --format='%B%x00' "$merge" | "$guards" conventional-bump)
  if [[ $asked == none ]]; then asked='-'; fi
  printf '    %s  %-5s  %s\n' \
    "$(git rev-parse --short "$merge")" "$asked" "$(git log -1 --format=%s "$merge")"
done < <(git rev-list --reverse "$released_range")
printf '\n'
printf '  Every line %s gains:\n' "$changelog"
printf '%s\n' "$section" | sed 's/^  *$//;s/^./    &/'
printf '\n'

read -rp 'Release this? Type Y to continue: ' confirmation
[[ $confirmation == Y ]] || refuse 'not confirmed'

unwind_to_reviewed_commit() {
  git reset --hard --quiet "$commit"
  git ls-files --error-unmatch "$changelog" >/dev/null 2>&1 || rm -f "$changelog"
}

prepare_release_commit() {
  git-cliff --config "$cliff_config" --tag "$version" --output "$changelog" || return 1
  "$guards" set-workspace-version "${version#v}" <Cargo.toml >Cargo.toml.release || return 1
  mv Cargo.toml.release Cargo.toml || return 1
  cargo update --workspace --offline --quiet || return 1
  git add -- "$changelog" Cargo.toml Cargo.lock || return 1
  git commit --quiet -m "chore(release): $version"
}

if ! prepare_release_commit; then
  rm -f Cargo.toml.release
  unwind_to_reviewed_commit
  refuse "the release commit could not be prepared -- $commit is restored, nothing was pushed"
fi
release_commit=$(git rev-parse HEAD)

# `warm-cache` is the gate plus the push, and it asserts the push actually
# landed -- cachix marks a rejected push with a red x and still exits 0. The
# image is in `checks` now, so the gate covers it and there is nothing to build
# here beyond it.
#
# It runs after the release commit rather than before it, because release.yml
# builds the tag, and the tag is on that commit -- warming the reviewed tree
# would warm a tree the runner never sees. The bill for that is a full
# dependency rebuild on every release: `Cargo.toml` is one of the flake's
# `rustSourceRoots`, so writing the crate version into it invalidates the
# vendored dependency derivation along with every crate above it.
printf '\nwarming the cache, so the release run is a hit...\n'
if ! warm-cache; then
  unwind_to_reviewed_commit
  refuse "the gate failed on the release commit -- $commit is restored, nothing was pushed"
fi

if ! git push --quiet origin "$release_commit:refs/heads/main"; then
  unwind_to_reviewed_commit
  refuse "pushing the release commit to main failed -- $commit is restored, no tag exists"
fi

# Asked again, of the state the push just produced. The earlier answers were
# about the reviewed commit and about a moment before anyone else's merge could
# land; these are about the commit the tag will name and a tag that a concurrent
# release may have taken since. The tag is still the last thing created, so a
# refusal here costs a release commit sitting on main untagged -- the version
# stays unspent and the next release carries that commit in its own range.
"$guards" assert-tag-unused "$version"
"$guards" assert-on-main "$release_commit"

git tag -a "$version" -m "Release $version" "$release_commit"
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
    --jq "map(select(.headBranch == \"$version\" or .headSha == \"$release_commit\")) | first | .databaseId // empty") || true
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
