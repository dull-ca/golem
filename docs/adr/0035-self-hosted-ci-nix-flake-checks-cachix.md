# 0035-self-hosted-ci-nix-flake-checks-cachix

## Status

Accepted 2026-07-28. The gate is implemented on `lakin/ci-nix-cachix`:
`nix flake check` runs the whole workspace test suite, the fleet harness, and
the six binary builds (`flake.nix` `checks`). The CI-box provisioning and the
cachix activation are **pending consequences**, not built here — tracked in
`docs/design/ci-cachix-nix.md` and `docs/TODO.md`. The release-publishing
mechanism is left **open** (see §5); it supersedes ADR 0028's Forgejo channel
without deciding a replacement.

## Context

Both repositories (golem, `emet.nvim`) moved from Codeberg to GitHub after
Codeberg changed its terms of service for LLM-generated code. `origin` is now
`git@github.com:dull-ca/golem.git`. Two things the prior CI depended on are
gone with the move:

- **Woodpecker** (`.woodpecker.yml`, `ci.codeberg.org`) — Codeberg's CI; it
  does not exist off Codeberg. The pipeline file is deleted.
- **The Forgejo/Codeberg release channel** ADR 0028 chose — the release API,
  the `FORGEJO_RELEASE_TOKEN` secret, the `dull/golem` release objects.

GitHub Actions is **rejected** (Dr. Dub, standing preference — he will not run
CI on a hosted CI product tied to the forge). The toolchain was already built
end-to-end by `flake.nix`; what CI adds over `nix build` is that it runs on
every push and caches the result so the next machine substitutes instead of
rebuilding.

The preceding implementer task (committed on this branch: `6e0c88d`, `81596f8`,
`1d79287`) already made `nix flake check` the complete gate and, doing so,
surfaced a latent repo defect on day one — the emet diagnostics corpus lived in
gitignored `.superpowers/sdd/errmsg/corpus/`, which nix's git-only source view
does not see, so 18 tests failed under nix while passing under bare `cargo`. The
fix moved the 64 corpus programs into tracked `apps/emet/tests/corpus/`. A gate
that runs the same way on every machine catches exactly this class of "works on
my disk" gap.

## Decision

### 1. One gate: `nix flake check`

The single CI gate is `nix flake check` on the repo flake. It runs the flake's
`checks`: `workspace-tests` (`cargo test` over the whole workspace),
`fleet-tests` (the `apps/fleet` python harness under `unittest`), and the six
binary builds (`golemd`, `golemctl`, `golemc-static` pair, `emetc`, `emet-lsp`)
inherited as checks. **CI is not a distinct system** — any machine with nix runs
the entire gate locally with the same command. There is no CI-only script to
keep in sync with local dev.

### 2. Self-hosted, golem-managed box (dogfood)

CI runs on a self-hosted box Dr. Dub owns, **provisioned by golem itself** — the
box's poll-build-push loop is authored as a fleet of golem's own four glyphs (a
`file` glyph for the loop script, a `systemdService` for the timer/service,
`file`/`lineInFile` for the cachix token). golem provisioning its own CI is the
dogfood. Not a hosted CI product; GitHub Actions rejected (§Context), Woodpecker
gone.

### 3. cachix is the binary cache

The CI box builds the flake outputs and pushes the resulting store paths to a
cachix cache (`cachix watch-exec <cache> -- nix build …`). Dev machines and
future CI runs substitute from that cache instead of rebuilding golem's crates
and its nixpkgs closure. **Activation is pending** two things that do not exist
yet: the cache itself (Dr. Dub is creating the account) and a `nixConfig`
follow-up in `flake.nix` (`extra-substituters` / `extra-trusted-public-keys`)
that can only be written once the cache name and public key exist — a
placeholder there produces trust warnings on every `nix` command, so it is
deliberately deferred. See `docs/design/ci-cachix-nix.md`.

### 4. The impure website build stays outside the gate

`website-container` needs `--impure` and an externally built `dist/` (Astro pulls
~137 platform-split native packages that `buildNpmPackage` can't reproduce), so
it is not in `checks` and not run by `nix flake check`. It is a scripted CI-box
step: `bun run build`, then `nix build --impure .#website-container`. This is the
existing `flake.nix` rationale, unchanged.

### 5. Releases: policy survives, channel and mechanism are open

ADR 0028's **channel is dead** — the Forgejo/Codeberg release object, its API,
and the `FORGEJO_RELEASE_TOKEN` path went away with the move to GitHub. ADR 0028
is superseded by this ADR.

The release **policy** carries forward unchanged: tag-driven (one git tag on the
shared `[workspace.package]` version names one coherent toolchain release), the
same artifact set (static-musl `golemd`/`golemctl`, native `emetc`, optionally
`emet-lsp`), and crates.io remains an explicit non-goal (the wire format stays an
internal, `format_version`-guarded contract).

The publish **mechanism is an open question — this ADR does not decide it.**
Candidates: GitHub Releases, pushed from the self-hosted box on a tag (no GitHub
Actions — the box's own loop does the upload); or serving the artifacts from
Dr. Dub's own infrastructure. Recorded open, tracked in `docs/TODO.md`.

## Consequences

- Any contributor with nix reproduces the full CI gate locally with one command;
  there is no privileged CI environment to reverse-engineer. A green
  `nix flake check` on a dev machine is the same green CI produces.
- The gate is only as complete as the git-tracked source, by construction —
  nix builds from git's view, so a test depending on an untracked file fails in
  CI even when it passes on the author's disk. This caught the corpus defect on
  day one (§Context) and will keep catching that class.
- Standing up the CI box, creating the cache, and wiring `nixConfig` are the
  pending activation steps — the gate is live, the automation around it is not
  yet. Until the box exists, `nix flake check` is run by hand.
- **Foreclosed:** no hosted CI product in the loop; no forge-coupled release
  token; no new resource kind (the CI box is provisioned by the same four glyphs
  as any other host). The release channel is deliberately left unbuilt rather
  than swapped one-for-one — picking GitHub Releases vs. self-hosted artifacts is
  its own decision.

## Alternatives considered

- **GitHub Actions.** Rejected by Dr. Dub outright — the reason the CI is
  self-hosted at all. A hosted CI product recreates the forge coupling the move
  off Codeberg was meant to shed.
- **Keep Woodpecker.** Not available off Codeberg; it was Codeberg's CI.
- **A hosted binary cache other than cachix** (e.g. an S3-backed `nix copy`
  store). cachix is the least-effort push/substitute path for a public cache and
  needs no infrastructure of Dr. Dub's beyond an auth token; a self-hosted store
  can be reconsidered if the release question resolves toward self-hosting
  everything.
- **Swap ADR 0028's Forgejo release for GitHub Releases as a decided default.**
  Declined here — recorded as an open question (§5) rather than decided, since
  the self-hosted-vs-GitHub-Releases trade-off is real and unforced today.

## Cross-references

- `flake.nix` — the `checks` output (the gate) and the impure `website-container`
  exclusion.
- `docs/design/ci-cachix-nix.md` — the concrete cachix + CI-box sketch this ADR
  decides in principle.
- ADR 0028 — superseded (the Forgejo release channel); its release *policy* is
  carried forward by §5's open release question.
- `docs/TODO.md` §C — the CI / publishing backlog (box, cachix activation,
  release mechanism, codeberg-reference sweep).
