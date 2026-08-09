# 0053 — Releases go out through a local guarded command

## Status

Accepted 2026-08-09. Narrows how ADR 0050's `v*` tag trigger is reached; does
not resolve ADR 0035 §5, where the release mechanism is still open.

Extended by ADR 0055 (2026-08-09), which derives the version from the
conventional commits instead of taking it from the command line, and lands a
changelog and crate-version commit on `main` before the tag. Every guard below
stands, and so does the property that a rejected release leaves nothing to undo
— up to the push of that commit, which is new ground the ordering below does not
cover.

Extended by ADR 0056 (2026-08-09), which moves `ci/release.sh` and
`ci/release-guards.sh` into the shared `dull-ca/nix` flake. "Both callers run
one file" still holds and is the reason for the move; the file is no longer in
this repository, and the published image name it also held now lives in
`ci/release-hooks.sh`.

## Context

Under ADR 0050 a `v*` tag is the whole release interface, and a tag is
unconstrained: any string, any commit, any number of times. Four tagging
mistakes in one day show what that costs — a tag on a mid-stream commit, two
tags on a commit that was never on `main`, one version string moved and re-run
so a published image was overwritten, and three tags for one release.

Every one is checkable before anything is built:

- the version reads `vMAJOR.MINOR.PATCH` or a prerelease of one,
- the tag does not exist yet,
- `ghcr.io/dull-ca/golem-docs:<version>` is not already published,
- the commit is an ancestor of `origin/main`.

Where those checks run decides the design. A `workflow_dispatch` job that
creates the tag cannot hand off to the `push: tags` job that publishes: **events
triggered by `GITHUB_TOKEN` do not create workflow runs**, excepting
`workflow_dispatch` and `repository_dispatch`, so that the token cannot recurse
([GITHUB_TOKEN](https://docs.github.com/en/actions/concepts/security/github_token)).
A dispatch path would have to duplicate the publish, and hold `contents: write`
to push a tag, to reach a state a developer's own tag push reaches for free.

## Decision

**`release` — a devenv script, `ci/release.sh` — is the way a release starts.**
It runs the four checks, prints the commit and version and waits for a literal
`Y`, builds the gate locally so the run is a cache hit, then tags and pushes.
The push carries the developer's credentials, so it triggers `release.yml`
normally. There is no `workflow_dispatch`: the local command already does what
one would, without a second copy of the publish or a wider token.

**`release.yml` keeps `on: push: tags` and re-runs the checks it still can.** A
tag pushed by hand bypasses `ci/release.sh` entirely, so the workflow re-checks
the version format, the ancestor, and the registry before the gate. It cannot
check that the tag is unused — by then it exists.

**Both callers run one file, `ci/release-guards.sh`.** A second copy of the
version pattern or the ancestor check would drift, and a drifted guard is worse
than none because it is trusted. The script also holds the published image name,
so the reference a guard inspects is the reference the publish writes.

**A failed run does not delete its tag.** Nothing publishes until the gate is
green, so the tag is inert, and the version is spent. The failure names the run
URL and prints the `git push origin --delete` for Dr. Dub to run himself.

## Consequences

- A release needs `gh` authenticated, because the script waits on the run and
  reports its verdict. An unauthenticated `gh` refuses before the tag exists
  rather than skipping the wait and reporting a success it never saw.
- Nothing is created or pushed until every check passes, so a rejected release
  leaves no tag, no image, and no half-state.
- A version consumed by a failed run cannot be reused. That is the tag guard
  working, not a defect; the answer is the next version.
- Releasing needs a clean checkout of `main` at `origin/main`. Releasing from a
  branch, a dirty tree, or a stale clone is refused before the checks run.
- The local gate run duplicates work the runner repeats. It is a cachix push
  followed by a cachix hit, and it is what makes a red gate cost no tag.
- `ci/release-guards.test.sh` covers the version pattern, the bump, and the
  image name. The git and registry checks are not covered — they need a repo and
  a network — and are exercised by running them.
