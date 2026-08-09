# 0052 — The docs image serves with a TLS-less static nginx

## Status

Accepted 2026-08-09. Amends the image built by ADR 0050; that record's
publishing decision is untouched.

## Context

The docs image ran caddy. Nothing chose it: `1d00e89` added
`sites/website/Containerfile` on `caddy:2-alpine` to get the tutorial's site
onto a box, and `mkWebsiteContainer` copied that shape into nix when the image
became a published artifact. No record weighs it.

`dull-ca/nix` builds `nginx-static-no-tls` for exactly this job, and
`dull-ca/dull.yyc.dev` already serves its Starlight site with it. golem already
depends on that flake — `buildBunPackage` comes from its overlay — so the
package costs an attribute reference.

Two measurements separate the two servers.

**Closure.** caddy is 89,262,752 bytes; `nginx-static-no-tls` is 1,686,352 — a
53× difference, on an image whose payload is a 2.6 MB static site.

**What each one can be made to do.** caddy's TLS is off by configuration
(`auto_https off`). It ships an ACME client, a certificate cache and a TLS
stack, and a one-line config edit or a stray `--config` puts a public-facing
HTTPS listener and a cert-fetching robot on a box that is supposed to be behind
Traefik. `nginx-static-no-tls` is compiled without `ngx_http_ssl_module`; an
`ssl` listener is a config error, and dull-nix has a check asserting nginx
rejects one.

The image sits behind Traefik and must never be reached directly. For that
placement, incapable beats configured-not-to.

## Decision

**Build the docs image from `dull-nix`'s `nginx-static-no-tls`**, configured by
`sites/website/nginx.conf`, and drop caddy from `flake.nix`.

**Run as `nobody` on `:8080`.** An unprivileged user cannot bind `:80` without
`CAP_NET_BIND_SERVICE`, and a privileged port buys nothing behind a proxy. Two
consequences of an empty base image follow from it: `dockerTools.fakeNss`
supplies the `/etc/passwd` entry that lets `nobody` resolve, and `extraCommands`
creates a 1777 `/tmp` for nginx's pid and client-body files — a `chmod` in the
`runCommand` would be reverted by nix's store canonicalisation, which resets
directory modes to 0555.

**`website-serves` asserts the behaviour, not the config.** It runs the real
nginx against the shipped `nginx.conf` and the real built site and checks four
things a reader would notice breaking: `/` answers 200 with `text/html;
charset=utf-8`, a missing path answers **404 *and* Starlight's styled
`404.html`**, a hashed stylesheet arrives gzipped as `text/css; charset=utf-8`,
and a directory URL without its trailing slash 301s to the canonical one.

**The caddy config stays for the tutorials.** `sites/website/Containerfile` and
`Caddyfile` build the fleet tutorials' image on a golem-managed box from a
docker base. That is a different artifact with a different purpose, and ADR 0023
treats a reverse proxy as a fleet `Workload` in its own right.

## Consequences

- The published image drops from 31.6 MB to 1.5 MB compressed (9.6 MB
  uncompressed on a docker host). Every pull, every layer push, every cachix
  round trip pays that difference.
- No ACME client, no TLS stack, and no admin API reach the box. The failure mode
  where a misconfigured docs container starts soliciting certificates is gone by
  construction rather than by a config line someone must not edit.
- **`zstd` encoding is lost.** The Caddyfile said `encode gzip zstd`; this nginx
  has no zstd module. gzip is what every client in practice negotiates, and the
  compressed sizes are within a few percent.
- Content types now match caddy's byte-for-byte only because `nginx.conf` says
  so. nginx's default `charset_types` exclude `text/css`, and its `mime.types`
  calls JavaScript `application/javascript` where caddy says `text/javascript` —
  both are valid, and the second difference is left standing.
- The trailing-slash 301 needs `absolute_redirect off`. nginx builds that
  redirect's `Location` from `listen`, not from the request's Host, so behind
  Traefik an absolute one would point a reader at `:8080` on the public
  hostname. Relative Locations resolve against what the client actually asked
  for.
- Two server configs now live in `sites/website/`. Each names the other and the
  artifact it belongs to, but a reader editing "the site config" can still pick
  the wrong one.
- **Conditional-request handling is untouched and still wrong.** Every file in
  the image carries nix's epoch mtime, so `Last-Modified` never advances and a
  client revalidating with `If-Modified-Since` can be told `304` after a
  release that changed the bytes. caddy had the identical defect, so this is not
  a regression — `dull.yyc.dev`'s `etag off` / `if_modified_since off` pair is
  the known fix, and adopting it is a separate decision about caching.
- golem and `dull.yyc.dev` now serve their sites the same way. A fix to the
  nginx build lands once, in `dull-ca/nix`, and both sites' checks catch a
  regression in it.
