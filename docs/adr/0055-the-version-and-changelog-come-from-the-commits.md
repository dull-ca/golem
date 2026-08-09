# 0055 — The version and the changelog come from the commits

## Status

Accepted 2026-08-09. Extends ADR 0053, whose guards, single guard file, and
refuse-before-anything-is-created property stand unchanged, and supersedes the
sequence in its decision: `release` no longer goes from the confirmation
straight to the tag. Runs inside ADR 0035's gate; the tag it produces still
means what ADR 0050 says it means.

Extended by ADR 0056 (2026-08-09), which moves the guards and the release
sequence into the shared `dull-ca/nix` flake. Every derivation rule below is
unchanged. "`ci/release-guards.sh` stays the only copy" is superseded: the
subcommands it names are `dull-nix`'s `releaseGuards`, `workspace-version` and
`set-workspace-version` are spelled `cargo-workspace-version` and
`set-cargo-workspace-version` there, and what stayed in golem is
`ci/release-hooks.sh` and `cliff.toml`.

## Context

ADR 0053 checked everything about a release except the one thing a release is
*for*. The guards read the version's shape — `vMAJOR.MINOR.PATCH`, unused,
unpublished, on `main` — and nothing reads its content, so `release v9.9.9` over
a range holding one typo fix passes all four. The four tagging mistakes 0053 was
written against were each a string typed at the wrong moment, and a hand-typed
version is the same kind of thing with the same failure left in it.

The content is derivable now. `main` is squash-merged, so every commit on it is
one pull request and its subject is a conventional-commit header —
`feat(website): give the docs site the favicon its head already links (#19)`.
The range between two stable tags is therefore a list of pull requests that
already state what they were.

Nothing said what changed between two releases. golem publishes a docs image
tagged by version and had no changelog at all; the tag was the entire record.

Four constraints shape what may be derived:

- Below `1.0`, Cargo already reads a minor bump as the incompatible one — `0.3`
  and `0.4` are separate compatibility ranges. A `major` derived from a `!` has
  nothing left to say that `minor` does not, and `v1.0.0` is the one version
  string that is a claim about stability rather than a description of a change.
- What a release publishes is the docs image. A range of nothing but `docs:` is
  a range with something to ship, so the conventional types that carry no code
  cannot be the ones that carry no release.
- A version written into `Cargo.toml` has to be committed before the tag,
  because the tag names a commit. A tag reading `v0.3.2` on a tree reading
  `0.1.0` is a contradiction any reader can find.
- git-cliff renders the whole file from git history every time. The changelog is
  therefore an output, and any edit to it is an edit to the wrong artifact.

## Decision

**The version follows from the conventional commits since the latest stable
tag.** A `feat:` in the range asks for a minor; any other conventional type asks
for a patch; a `!` in the header or a `BREAKING CHANGE:` footer asks for a
major. The loudest wins.

- **Every conventional type is worth at least a patch**, not only `feat` and
  `fix`, because the artifact is the docs image and a `docs:`-only range still
  has something to publish.
- **A derived `major` below `1.0` is served as a `minor`.** `release major`,
  typed deliberately, is the only path to `v1.0.0`.
- **A range in which no subject is conventional is refused, not guessed at.** A
  patch would be a version nobody chose, taken from subjects that are also about
  to become the changelog. Rewording them, or naming the version, is the way
  past it.
- **`release major|minor|patch` overrules the bump and `release vX.Y.Z` the
  whole derivation.** Prereleases have no derivation and are always named.

**Every merge the version was read from is shown before the question is asked**,
each with the bump its subject asked for. A squash subject is typed by hand in
the merge box and is all that survives of the pull request behind it — `chore:
updated docs and readme (#20)` carried a systemd unit fix — and the same words
become the changelog line. Confirming is the only place that can be caught.

**A release commit lands on `main` before the tag.** One
`chore(release): vX.Y.Z` carries the rendered `CHANGELOG.md`, the workspace
version in `Cargo.toml`, and the relocked `Cargo.lock`; `warm-cache` runs the
gate on *that* commit, because that is the tree the runner builds; the commit is
pushed to `main`; the guards are asked again about the state the push produced;
and the tag goes on last. A failure before the push restores the reviewed commit
exactly, so ADR 0053's property holds up to the point where something exists.

**`cliff.toml` is the changelog format, and it makes no network call.** The
`(#N)` a squash subject already carries is rewritten into a pull-request link;
`[remote.github]`, which resolves the same links over the API, would put a token
and a request inside a release for a number the subject is holding.

**`ci/release-guards.sh` stays the only copy.** `latest-stable`,
`conventional-bump`, `effective-bump`, `workspace-version`, and
`set-workspace-version` are subcommands of it, covered by
`ci/release-guards.test.sh` on the same terms as the guards beside them.

## Consequences

- The version is a reading of the range rather than an assertion about it, and
  the bump is auditable before `Y`: the merge list shows which subject produced
  it. The cost is that subjects now carry weight they did not before — a
  mistyped type is a wrong version, catchable only by reading.
- `v1.0.0` cannot happen by accident, and cannot happen at all until someone
  means it.
- Releasing pays a full dependency rebuild. `Cargo.toml` is one of the flake's
  `rustSourceRoots` (ADR 0054), so writing the crate version into it invalidates
  the vendored dependency derivation along with every crate above it. Warming
  the reviewed commit instead would be cheap and would warm a tree the runner
  never checks out.
- A release pushes to `main`, so it starts `ci.yml` as well as `release.yml`.
  Two runs per release, one of them redundant with the gate `warm-cache` just
  ran and pushed.
- A failure between the push to `main` and the tag leaves the release commit on
  `main` untagged. The version is unspent and the next `release` carries that
  commit in its own range — `cliff.toml` skips `chore(release)`, so it
  contributes a patch and no changelog line. This is the one window where
  ADR 0053's "a refusal leaves nothing to undo" no longer holds, and it is
  narrowed to two guard calls.
- `v0.1.0` and `v0.2.0` have no changelog section and never will. Both tags
  point at `fd603b2`, which is not an ancestor of `origin/main` — mis-tags from
  the day ADR 0053 was written — so there is no range for git-cliff to walk
  between them, and the commits they were meant to mark fold into `v0.3.0`.
  `CHANGELOG.md`'s header says so rather than leaving the gap unexplained.
- `cliff.toml` is not in the gate. A broken config fails when `release` renders
  the section it shows you, which is before the confirmation and before anything
  is created — the failure is loud and costs nothing, but `nix flake check` will
  not find it first.
- The crate version is `[workspace.package]` alone. `libs/workspace-hack` keeps
  its own and is left behind deliberately (`publish = false`, regenerated by
  `cargo hakari generate`); `flake.nix` pins `commonArgs.version = "0.1.0"` for
  the derivation name, which nothing reads and nothing updates, so it is a
  second written version that will drift.
- The first release moves the workspace version from `0.1.0` to the tag, since
  the tags ran ahead of `Cargo.toml` before anything wrote it.
