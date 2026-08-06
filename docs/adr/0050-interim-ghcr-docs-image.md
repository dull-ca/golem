# 0050 — The docs image ships through ghcr.io, for now

## Status

Accepted 2026-08-06. Interim under ADR 0035 §5, which it does **not** resolve.
Mirrors `dull-ca/dull.yyc.dev`'s ADR 0001, which made the same call for the same
reasons.

## Context

dulliac serves golem's documentation at `golem.yyc.dev` from `dull-01`, pulling
`ghcr.io/dull-ca/golem-docs:latest`. Nothing published that image.

The artifact already existed: `flake.nix`'s `mkWebsiteContainer` builds a
`dockerTools.buildLayeredImage` holding caddy, the built site under
`/var/www/html`, and `sites/website/Caddyfile`. What was missing was a channel
from a build to a machine that pulls.

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

**The smoke test is the gate that matters.** `nix flake check` validates
configuration in a sandbox with no docker daemon, so it cannot show the
assembled image serves anything. The smoke test loads the archive, runs it, and
asserts 200 and an HTML content type on `/` plus 404 on a path that does not
exist.

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
- CI now runs bun and docker, so the gate is no longer `nix flake check` alone —
  a runner without a docker daemon cannot run it. That is the cost of testing
  the real artifact instead of its configuration.
- `nix flake check` runs **before** the site build in both workflows. `siteDist`
  falls back to an in-tree `sites/website/dist`, so building first would pull
  `website-container` into `packages` and quietly change what the check covers
  depending on step order.
- No TLS in the container. Traefik terminates in front of it, which is why the
  Caddyfile already sets `auto_https off`.
