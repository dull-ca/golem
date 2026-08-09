# 0056 — The release command is a shared flake input

## Status

Accepted 2026-08-09. Extends ADR 0053 and ADR 0055, whose guards, version
derivation, changelog rendering, and ordering all stand unchanged. It supersedes
one sentence in each: ADR 0053's "Both callers run one file,
`ci/release-guards.sh`" and ADR 0055's "`ci/release-guards.sh` stays the only
copy". There is still one copy; it is no longer in this repository. Runs inside
ADR 0035's gate.

Amended 2026-08-09: `dull-ca/nix` is pushed and golem's `flake.lock` moved
`983d932` → `edd816e`, closing the gap the first consequence records. The
dependency that consequence describes is permanent; only the red gate was
temporary.

## Context

ADR 0053 and ADR 0055 built three files — `ci/release.sh`,
`ci/release-guards.sh`, `ci/release-guards.test.sh` — and almost nothing in them
is about golem. The version pattern, the conventional-commit reading, the
pre-1.0 demotion, the merge list shown before the confirmation, the release
commit landing on `main` ahead of the tag, the re-asked guards after the push:
every one of those is a property of releasing a repository whose `main` is
squash-merged, not a property of golem.

What *is* golem's is small and enumerable: the docs image is the artifact,
`skopeo inspect` is how "already published" is asked, `Cargo.toml` and
`Cargo.lock` are the version-bearing files, `release.yml` is the workflow to
watch, and `warm-cache` is the gate to run first.

dull.yyc.dev wants the same release flow. Copying the three files there is
exactly the drift ADR 0053 refused within one repository — "a second copy of the
version pattern or the ancestor check would drift, and a drifted guard is worse
than none because it is trusted" — widened to two repositories, where the two
copies cannot even be diffed in one checkout.

`dull-ca/nix` is already a flake input here, for `buildBunPackage` and
`nginx-static-no-tls`, on the reasoning that a fix to either should land once.

Three constraints shape what can be shared:

- The repo-specific half has to arrive without the shared half naming any of it.
  A driver that stages `Cargo.toml` is a driver that has learned Rust.
- The guards decide what may be published, so a consumer has to be able to gate
  them itself. dull-ca's own `nix flake check` ran them at dull-ca's revision
  under dull-ca's nixpkgs, which is neither the revision nor the nixpkgs a
  consumer resolves.
- A release must be reproducible from the lockfile. `nix run github:dull-ca/nix`
  would resolve whatever `main` held at the moment someone typed it.

## Decision

**The release driver, the guards, and the guard suite move to `dull-ca/nix`.**
They are exposed as three overlay members: `mkReleaseCommand` (which builds
`bin/release`), `releaseGuards` (`bin/release-guards`, for the CI job that
re-checks a hand-pushed tag), and `releaseGuardsTest` (the guard suite as a
derivation).

**golem keeps `ci/release-hooks.sh` and `cliff.toml`,** and wires them in through
`flake.nix`. Those two files are golem's entire half of a release.

**The hooks contract is four subcommands, all mandatory:** `assert-ready`,
`describe VERSION`, `assert-unpublished VERSION`, `set-version VERSION`. A
repository supplying one supplies all four; a repository with nothing to check
writes the no-op out. Dispatch cannot distinguish "nothing to do here" from "the
case fell through", and a publish guard that silently does not run is worse than
one that was never written.

**`set-version` prints the paths it wrote, one per line, and the driver stages
those.** The driver names no file but `CHANGELOG.md`, because `CHANGELOG.md` is
the only file it writes. golem's hooks print `Cargo.toml` and `Cargo.lock`; a
node package prints `package.json`; a repository that versions nothing on disk
prints nothing and gets a release commit carrying the changelog alone.

**The release branch and the changelog filename are not parameters.** `main` and
`CHANGELOG.md` have one defensible value each, and an option every caller passes
the same argument to only adds a way to be wrong.

**golem gates `releaseGuardsTest` in its own `checks`,** at the dull-nix revision
its lockfile pins, rather than trusting dull-ca's gate. The guards are the same
text; the bash, awk, sort and grep under them are not.

**dull-nix stays a pinned flake input.** A release built from the lockfile is a
release someone can reconstruct.

## Consequences

- **The gate is red at this commit, and a push plus a lock update is the only
  fix.** `.#release` and `.#release-guards` do not evaluate:

  ```
  error: attribute 'mkReleaseCommand' missing
         at flake.nix:240:19
  ```

  `dull-ca/nix`'s release work is uncommitted and unpushed, and golem's
  `flake.lock` pins `dull-nix` at `983d932`, which predates it. Both outputs are
  in `checks`, so `nix flake check` fails, so `ci.yml` fails; `nix flake show`
  and `release` fail with them. `nix build` and the individual crate and site
  outputs are unaffected — laziness spares anything that does not reach the
  attribute. Two steps, in this order: push `dull-ca/nix`, then `nix flake
  update dull-nix` here. There is no change to golem that substitutes for
  either; the attribute does not exist at the pinned revision. Both steps are
  done as of 2026-08-09 (see Status); what follows is what any later stale lock
  will cost.
- **golem's release path now depends on another repository, and `flake.lock` is
  the whole of that dependency.** A lock pointing at a dull-nix without
  `mkReleaseCommand` is not a degraded release — it is an output that does not
  evaluate, which under ADR 0035 is the entire gate failing rather than one
  check going red. The failure is loud, which is the good half; the bad half is
  that it is not fixable from inside this repository.
- **A fix to the guards does not reach golem on its own.** golem has no
  `update.yml`; `dull-nix` moves when someone runs `nix flake update` here and
  not otherwise, so the shared copy can be ahead of the one golem releases with
  for as long as nobody looks. The reverse risk — an update arriving that
  nobody vetted — is what `release-guards-hold` in golem's own `checks` covers:
  whatever revision the lock moves to, the suite runs against it under golem's
  nixpkgs before the gate goes green.
- **Changing a guard is now two repositories and two reviews**, and the change is
  not live in golem until a lock update lands. That is slower than editing
  `ci/release-guards.sh` was, and it is the cost of the single copy being single
  across repositories rather than within one.
- **ADR 0053's "the script also holds the published image name" no longer
  describes the tree.** The image name is in `ci/release-hooks.sh` now. The
  property it was defending survives intact: `release.yml` and `release` still
  run one file for `assert-unpublished` and for the reference the publish
  writes, so the reference a guard inspects is still the reference the publish
  writes.
- **A cold store with no network cannot build `release`.** Previously
  `ci/release.sh` was a file in the checkout and ran with bash alone.
- ADR 0035 §5 is untouched. Where artifacts are served from is still open, and
  this record is about where the release command lives, not where it publishes.
