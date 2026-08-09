# 0050 — The docs image ships through ghcr.io, for now

## Status

Accepted 2026-08-06. Interim under ADR 0035 §5, which it does **not** resolve.
Mirrors `dull-ca/dull.yyc.dev`'s ADR 0001, which made the same call for the same
reasons.

## Context

dulliac serves golem's documentation at `golem.yyc.dev` from `dull-01`, pulling
`ghcr.io/dull-ca/golem-docs:latest`. Nothing published that image.

The artifact already existed: `flake.nix`'s `mkWebsiteContainer` builds a
`dockerTools.buildLayeredImage` holding a web server, the built site under
`/var/www/html`, and its config. What was missing was a channel from a build to
a machine that pulls.

That server was caddy on `:80` when this was written; ADR 0052 replaced it with
a TLS-less static nginx on `:8080`. Nothing here turns on which one it is.

ADR 0035 §5 left the release mechanism open. Nothing has closed it, and a
consumer waiting on a URL is not a reason to close it badly.

## Decision

**Publish to `ghcr.io/dull-ca/golem-docs`, and say in the workflow that this is
interim.** The registry is where dulliac already looks, `GITHUB_TOKEN` already
authenticates to it, and the image is public so `dull-01` pulls anonymously with
no credentials and no `registries.conf.d` fragment.

**Two workflows, not one.** `ci.yml` builds and smoke-tests on every push and
pull request and never publishes, with `permissions: contents: read` so its
token cannot publish whatever the repository default grants. `release.yml`
triggers on `v*` tags and publishes.

**`release.yml` re-runs the whole gate rather than trusting `ci.yml`.** A tag can
point at any commit; nothing constrains it to one CI already passed. Borrowing
that verdict would let a tag on an unverified commit publish straight to
`:latest`.

**Only `vMAJOR.MINOR.PATCH` moves `:latest`.** A `v1.2.3-rc1` tag matches the
`v*` trigger and publishes under its own version tag alone, because `:latest` is
what an unpinned pull gets and `dull-01` pulls unpinned.

**The published name is `golem-docs`; the image's internal name stays
`golem-website`.** The destination ref in the skopeo copy is what a consumer
reads, and it does not have to match what `mkWebsiteContainer` calls the image.

**The site builds in nix, and so does the test.** `websiteNodeModules` is a
fixed-output derivation running `bun install`; `websiteDist` patches the
prebuilt ELF binaries npm ships (`autoPatchelfHook`) and their `/usr/bin/env`
shebangs (`patchShebangs`), then builds. `website-serves` runs the real server
against the real config and the real built site and asserts what a reader gets.
Both are in `checks`, so `nix flake check` covers them.

ADR 0035 §4 said the dist could not be produced purely. That was wrong: the
obstacle was never reproducibility, it was that npm's binaries link against a
loader the nix store does not have. `autoPatchelfHook` is the standard answer
and it took two lines. **§4 is superseded by this record.**

## Consequences

- dulliac gets a URL it can pull today, without golem deciding its release
  mechanism under deadline.
- **ADR 0035 §5 stays open.** Both workflows say so at the top, and this record
  exists so a future reader finds a decision rather than inferring one from a
  registry name.
- Publishing depends on GitHub — the thing ADR 0035 wants to stop depending on.
  The dependency is one `skopeo copy` destination argument; everything upstream
  of `docker-archive:result` is unaware ghcr.io exists, so retargeting is an
  edit to two lines rather than a rewrite.
- The image must stay public. A private one would need `dull-01` to hold
  registry credentials, which is the trust boundary ADR 0042 keeps deliberately
  small.
- CI needs nothing but nix. No bun, no docker daemon, no `--impure`, no
  `GOLEM_SITE_DIST`, and no shell script standing in for a test — `nix flake
  check` is the gate again, and it runs the same on a laptop as on a runner,
  which is what ADR 0035 wanted in the first place.
- `websiteNodeModules` carries a hash over `package.json` and `bun.lock`.
  Changing either changes the hash, and the build fails until it is updated —
  nix reports the correct one. That is the cost of a network fetch being
  pinned rather than trusted.
- npm ships every platform's sharp, so the musl builds land in the store and
  can never link against a glibc loader. They are ignored explicitly
  (`autoPatchelfIgnoreMissingDeps`) rather than silently, because the glibc
  variants beside them are what actually loads.
- `website-serves` rewrites the config's sandbox-impossible paths — the document
  root among them — because a build sandbox cannot create `/var/www/html`. Every
  other directive is the file as shipped.
- No TLS in the container. Traefik terminates in front of it; under ADR 0052 the
  server cannot serve TLS even if asked.
