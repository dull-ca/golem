# 0028-tag-driven-binary-releases

## Status

Superseded by 0035 (2026-07-28).

Both repositories moved from Codeberg to GitHub, so the **channel** this ADR
chose no longer exists: there is no Forgejo/Codeberg release object, no
`ci.codeberg.org` Woodpecker pipeline, and no `FORGEJO_RELEASE_TOKEN` activation
path. The Decision below is preserved as the record of that choice.

The release **policy** — tag-driven versioning off the single shared
`[workspace.package]` version, the same artifact set (static-musl
`golemd`/`golemctl`, native `emetc`, optional `emet-lsp`), and crates.io as an
explicit non-goal — is carried forward by ADR 0035, which leaves the replacement
publish mechanism an open question rather than swapping one channel for another.

## Context

The toolchain is already built end-to-end by `flake.nix` and exercised on every
push/PR by `.woodpecker.yml`, but nothing is *published*: CI proves the binaries
compile and then discards them. There is no way for a user (or a Debian guest
being provisioned) to obtain a `golemd`/`golemctl`/`emetc` without cloning the
repo and running `nix build` themselves. A release channel is the missing step.

Ground truth confirmed against the current tree:

- **Flake outputs (`flake.nix`).** The exact `packages` set is `golemd`,
  `golemctl`, `golemd-static`, `golemctl-static`, `emetc` (pname `emet`),
  `emet-lsp`, `tree-sitter-emet`, `website-container`, with `default = emetc`.
  The `*-static` pair is built with `pkgsStatic` — "musl libc, bundled sqlite,
  rustls/ring crypto into one file" — precisely because "a nix-dynamic binary
  links its interpreter as a `/nix/store` path, so it can't run off NixOS." The
  non-static `golemd`/`golemctl`/`emetc`/`emet-lsp` are ordinary nixpkgs-dynamic
  builds, runnable on NixOS or under `nix run`, but **not** portable onto a
  stock Debian guest.
- **CI (`.woodpecker.yml`).** Runs on `event: push` and `event: pull_request`
  only. It already `nix build`s `.#golemd .#golemctl .#emetc` and, in a separate
  step, `.#golemd-static .#golemctl-static`. It runs on `ci.codeberg.org` (the
  header comment and the caching notes name it). No secret is configured today
  ("No secret (active)" in the caching notes); a future `CACHIX_AUTH_TOKEN` is
  the only secret contemplated, and it is deferred. The website container is
  **built, not pushed** — "Image push waits on a registry."
- **Remote (`git remote -v`).** `origin` is
  `ssh://git@codeberg.org/dull/golem.git` — Codeberg/Forgejo, matching the site's
  social link (`codeberg.org/dull/golem`) and the CI host. The branch carrying
  this ADR is **local and unpushed**: the host has no `dull/golem` content to
  attach a release to yet.
- **Versioning (`Cargo.toml`).** The workspace sets a **single shared**
  `[workspace.package] version = "0.1.0"`, and every `flake.nix` output hardcodes
  `version = "0.1.0"` to match. There is one version for the whole toolchain, not
  per-crate versions — so one git tag names one coherent release.
- **License.** A `LICENSE` file **exists** at the repo root, and it is **GNU
  AGPLv3**. But `[workspace.package] license = "MIT OR Apache-2.0"`, inherited by
  every crate's `Cargo.toml`. The declared SPDX metadata and the actual license
  text **disagree** — this must be reconciled before binaries are distributed
  (§License).
- **The wire format is an implementation detail (root `CLAUDE.md`).** "The
  manifest is binary postcard today; … The model doesn't change, the serializer
  might. Don't elevate encoding details as the headline." The `scroll-format`
  crate is the shared writer/reader contract, deliberately *internal* — not a
  surface any external consumer is invited to depend on. This is the load-bearing
  reason crates.io publication is a non-goal (§crates.io).

## Decision

Publish **tag-driven binary releases**, built from the existing `flake.nix`
outputs and attached to a **Forgejo/Codeberg release** on each version tag. No
package registry, no library publication — just the compiled tools, versioned by
git tag.

### 1. Artifacts

Attach these built outputs to each release. State plainly which are portable and
which are not:

| Artifact | Flake output | Linkage | Audience |
|---|---|---|---|
| `golemd` | `.#golemd-static` | **static-musl** (portable) | the agent, deployed onto Debian guests |
| `golemctl` | `.#golemctl-static` | **static-musl** (portable) | the operator CLI, run against those guests |
| `emetc` | `.#emetc` | nixpkgs-dynamic (native) | the authoring compiler |
| `emet-lsp` | `.#emet-lsp` | nixpkgs-dynamic (native) | editor language server (**optional**) |

- **`golemd`/`golemctl` ship as the `*-static` builds.** They "run on Debian
  guests with no shared-lib matching" — that portability is the whole point of
  the `pkgsStatic` outputs, and the runtime target for these two is exactly a
  provisioned guest, which is not NixOS. Shipping the dynamic build for these two
  would produce a binary that only runs on the machine that built it.
- **`emetc` (and optionally `emet-lsp`) ship as the native (`pkgs`) builds.**
  These are *authoring-host* tools. There is no `emetc-static`/`emet-lsp-static`
  output in the flake today, and the authoring host is a developer machine (often
  NixOS, or running the tool via `nix run`), not a locked-down guest. If a
  portable `emetc` is later wanted for CI images or non-Nix authoring hosts, add
  an `emetc-static` output and promote it here — a clean follow-up, not a
  blocker. `emet-lsp` is optional in the first release; it can be added to the
  artifact list once an editor-distribution story exists.
- **Not shipped:** `tree-sitter-emet` (a build input for the LSP/highlighter, not
  an end-user binary) and `website-container` (its push "waits on a registry" —
  ADR-external, tracked by the CI comment, not here).

The static artifacts are the guest-facing contract; the native ones are the
authoring convenience. The release notes must label each so a user does not try
to run the native `emetc` on a bare Debian box.

### 2. Channel

Attach the binaries to a **git-host release** — a Forgejo/Codeberg *release*
object on the version tag — on `codeberg.org/dull/golem`. Codeberg runs Forgejo,
whose release API is Gitea-compatible; a release is created for a tag and binary
**assets** are uploaded to it. This is the channel because it is where the repo
already lives, where CI already runs, and it needs no third-party account or
registry. No GitHub mirror, no separate download host.

### 3. Versioning

- **Git tags, semver, off the Cargo workspace version.** The workspace has one
  shared `version` (`0.1.0` today) and every flake output pins it, so **one tag
  names one coherent toolchain release** — all four artifacts move together.
- The release tag is `v<version>`, e.g. `v0.1.0`, matching
  `[workspace.package] version`. Bumping a release = bump `[workspace.package]
  version` (and the mirrored `version` strings in `flake.nix`), commit, tag
  `v<new>`, push the tag.
- No independent per-crate versioning: the crates are not released independently
  (see §crates.io), so their versions do not diverge.

### 4. Trigger

A **Woodpecker CI job that fires on tag push** (`when: event: tag`) which
`nix build`s the release outputs and uploads them to the Forgejo/Codeberg
release via the host's release API, authenticated by a repo secret. This is the
single mechanism; there is no manual upload step in the steady state. The job is
**gated on a `FORGEJO_RELEASE_TOKEN` secret** Dr. Dub supplies in the Codeberg
repo's Woodpecker settings — absent the secret, the job cannot authenticate and
the release is not published, which is the intended safety interlock during the
pending-activation window.

### 5. crates.io — DEFERRED (explicit non-goal)

The workspace crates are **not** published to crates.io, now or as part of this
ADR. They are not designed as externally-consumed libraries:

- `scroll-format` is the *internal* writer/reader contract, and the root
  `CLAUDE.md` is explicit that "the wire format is an implementation detail" —
  publishing it as a crate would invite external code to depend on an encoding
  the project reserves the right to change under a `format_version` bump.
- `golemd`/`golemctl`/`emet`/`emet-lsp` are applications, delivered as binaries
  (§1), not as `cargo add`-able libraries.

**Reconsider only if** a genuine external library consumer appears — e.g. a
third party wants to *generate* or *read* manifests in their own Rust program
against a stabilized, versioned `scroll-format` API. That would be its own ADR
(stabilizing the crate's public surface, committing to semver on it, and
deciding the encoding-stability guarantee), superseding this section — not a
default.

### License

Publishing distributable binaries requires an unambiguous license, and the repo
is currently **inconsistent**: the `LICENSE` file is **GNU AGPLv3**, while every
crate's Cargo metadata declares `license = "MIT OR Apache-2.0"`. This must be
reconciled to **one** answer before the first release ships — a binary handed to
a user must state its actual terms.

**This is Dr. Dub's call**, not the ADR's to make silently. The recommendation,
flagged for decision:

- **If the AGPLv3 `LICENSE` is intended** (a strong-copyleft, network-clause
  license — a deliberate choice for an agent that provisions others' machines):
  change `[workspace.package] license` to `"AGPL-3.0-only"` so the SPDX metadata
  matches the file, and keep the `LICENSE` file.
- **If permissive was intended** (`MIT OR Apache-2.0`, matching the Cargo
  metadata and the common Rust-ecosystem default): replace the AGPLv3 `LICENSE`
  file with the dual `LICENSE-MIT` + `LICENSE-APACHE` texts the metadata already
  claims.

Either way the two must agree. The recommendation, absent a stated preference,
is to **make the metadata follow the file** (`AGPL-3.0-only`), since an explicit
AGPLv3 `LICENSE` text is the more deliberate artifact than an unedited
metadata default — but this is explicitly Dr. Dub's decision, and no license
text is changed by this ADR.

### Activation prerequisites (checklist)

The ADR is Accepted, but a first publish requires all of:

1. **Repo pushed to the host.** `dull/golem` must exist on `codeberg.org` with
   this branch's history, so a release object can be created against a pushed
   tag.
2. **`FORGEJO_RELEASE_TOKEN` secret** configured in the Codeberg repo's
   Woodpecker CI settings — a Forgejo access token with `write:repository`
   (release-creation) scope.
3. **License reconciled** (§License) — one coherent `LICENSE` + matching SPDX
   metadata, Dr. Dub's choice.

## Implementation plan

Concrete enough to build the CI job directly. Add a **new step** to
`.woodpecker.yml`, gated on `when: event: tag`, that reuses the existing nix
toolchain image and env.

**Step outline** (added alongside the existing push/PR `nix` step; the top-level
`when:` already lists `event: push` and `event: pull_request` — add
`- event: tag` there so tag pushes enter the pipeline, and gate the release step
itself on the tag event):

```yaml
# Add to the top-level when: so tag pushes run the pipeline.
when:
  - event: push
  - event: pull_request
  - event: tag

steps:
  # ... existing `nix:` step unchanged (test + build proof on every push/PR) ...

  release:
    image: *nix_image
    environment: *nix_env
    when:
      - event: tag          # only on a pushed version tag (e.g. v0.1.0)
    secrets:
      - forgejo_release_token
    commands:
      # 1. Build exactly the release outputs: static agent+CLI, native compiler.
      - nix build .#golemd-static .#golemctl-static .#emetc --print-build-logs --out-link result-release
      #    (optionally add .#emet-lsp here once it joins the artifact list)
      - mkdir -p dist
      - cp result-release*/bin/golemd  dist/golemd
      - cp result-release*/bin/golemctl dist/golemctl
      - cp result-release*/bin/emet     dist/emetc   # flake pname is `emet`; ship as `emetc`
      # 2. Create the Forgejo/Codeberg release for this tag and upload the assets
      #    via the Gitea-compatible release API. CI_COMMIT_TAG is the pushed tag.
      - |
        api="https://codeberg.org/api/v1/repos/dull/golem"
        # Create the release object for the tag (idempotent: ignore if it exists).
        rel=$(curl -sf -X POST "$api/releases" \
          -H "Authorization: token $FORGEJO_RELEASE_TOKEN" \
          -H "Content-Type: application/json" \
          -d "{\"tag_name\":\"$CI_COMMIT_TAG\",\"name\":\"$CI_COMMIT_TAG\"}" \
          | { command -v jq >/dev/null && jq -r .id || sed -n 's/.*"id":\([0-9]*\).*/\1/p'; })
        for f in golemd golemctl emetc; do
          curl -sf -X POST \
            "$api/releases/$rel/assets?name=$f" \
            -H "Authorization: token $FORGEJO_RELEASE_TOKEN" \
            -F "attachment=@dist/$f"
        done
```

Notes for the implementer:

- **Secret name.** Woodpecker lowercases secret references; declare
  `forgejo_release_token` under `secrets:` and read it as
  `$FORGEJO_RELEASE_TOKEN` in-shell. Configure the secret value in the Codeberg
  repo's Woodpecker settings (a Forgejo token with `write:repository` scope).
- **Flake outputs built:** `.#golemd-static`, `.#golemctl-static`, `.#emetc`
  (add `.#emet-lsp` when promoted). The `*-static` pair is what ships to guests;
  `emetc` is the native authoring build. Do **not** build the dynamic
  `.#golemd`/`.#golemctl` for release — they are not portable (§1).
- **Asset rename.** The `emetc` flake output has pname `emet` and produces
  `bin/emet`; ship the asset as `emetc` to match the tool's documented name.
- **Release API shape.** Codeberg exposes the Gitea/Forgejo API at
  `/api/v1/repos/{owner}/{repo}/releases` (POST to create, then POST to
  `/releases/{id}/assets?name=...` with a multipart `attachment` file). `curl` +
  `jq` keeps the step dependency-free inside the nix image (a `jq`-less fallback
  is shown); a Woodpecker Gitea-release plugin is an alternative if one is
  available on `ci.codeberg.org`.
- **Caching interplay.** The existing caching notes (single-step store reuse;
  the deferred `CACHIX_AUTH_TOKEN`) apply unchanged; the release step is a fresh
  container and will re-substitute from `cache.nixos.org` and rebuild golem's own
  crates, same as the push step. If/when cachix is activated, wrap the release
  `nix build` the same way.

## Alternatives considered

- **GitHub Releases (mirror to GitHub, release there).** Rejected as the default:
  the repo lives on Codeberg, CI runs on `ci.codeberg.org`, and the social link
  is `codeberg.org/dull/golem`. Releasing where the code already is needs no
  mirror, no second account, no cross-host token. A GitHub mirror + release could
  be *added* later for reach, but it is not the primary channel.
- **crates.io publication.** Deferred as an explicit non-goal (§crates.io): the
  crates are applications and an intentionally-internal wire-format library, not
  externally-consumed libraries. Reconsidered only under a stated external
  library consumer, via its own ADR.
- **A devenv / flake release task run manually.** Rejected as the steady-state
  mechanism: a human-run `nix build && curl-upload` task is un-auditable and
  easy to skip or misfire. Tying releases to a **pushed tag + CI** makes the
  release reproducible from the tag alone and removes the human upload step. (The
  Implementation-plan step is effectively that task, but owned by CI and gated on
  the tag event and the release secret.)
- **Publish the dynamic (non-static) golemd/golemctl.** Rejected: a nix-dynamic
  binary "links its interpreter as a `/nix/store` path, so it can't run off
  NixOS" — useless on the Debian guests that are golemd's runtime target. The
  `*-static` outputs exist precisely to avoid this.

## Consequences

- Users and provisioned guests can obtain versioned `golemd`/`golemctl`
  (portable static-musl) and `emetc` (native) from a Codeberg release, without
  cloning and building — the missing distribution step is closed.
- One git tag on the shared workspace version releases the whole toolchain
  atomically; there is no per-crate version skew to manage.
- Releases are reproducible from the tag: CI rebuilds the exact flake outputs, so
  a release asset is `nix build`-verifiable from the tagged source.
- **Foreclosed / held:** the wire format stays an implementation detail —
  nothing here publishes `scroll-format` as a consumable library or commits to
  encoding stability beyond the existing `format_version` guard. No new resource
  kind, no new registry dependency, no third-party host in the release path.
- **Costs / open items:** the first publish is blocked on the three activation
  prerequisites (push, secret, license). The license inconsistency (AGPLv3 file
  vs. `MIT OR Apache-2.0` metadata) is a real blocker requiring Dr. Dub's
  decision, not a formality. `emet-lsp` distribution and a portable `emetc-static`
  are deferred conveniences. The website container push remains out of scope
  (waits on a registry, tracked by the CI comment).
- This ADR is **ready for a CI-scaffold implementation pass**: the `release:`
  step above can be added to `.woodpecker.yml` now; it stays inert until a tag is
  pushed to a host that has the repo and the secret.

## Cross-references

- Root `CLAUDE.md` — "the wire format is an implementation detail"; the
  load-bearing reason `scroll-format` (and the crates generally) are not
  published to crates.io.
- `flake.nix` — the release outputs (`golemd-static`, `golemctl-static`, `emetc`,
  `emet-lsp`) and the static-vs-dynamic linkage rationale.
- `.woodpecker.yml` — the existing push/PR pipeline this extends; the caching
  notes and the single-step store-reuse constraint the release step inherits; the
  "build, not push" precedent for the website container.
- `Cargo.toml` — the shared `[workspace.package] version` a tag names, and the
  `license` metadata that must be reconciled with the `LICENSE` file (§License).
- ADR 0012 / 0013 — the binary content-addressed manifest and `scroll-format`
  crate whose internal, `format_version`-guarded encoding is why the crate is not
  published.
