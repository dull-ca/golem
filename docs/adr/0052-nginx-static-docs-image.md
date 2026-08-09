# 0052 — Both docs images serve with a TLS-less static nginx

## Status

Accepted 2026-08-09. Amends the image built by ADR 0050; that record's
publishing decision is untouched.

## Context

golem builds the docs site into two images. `flake.nix`'s `mkWebsiteContainer`
produces the one ghcr.io publishes; `sites/website/Containerfile` produces the
one the fleet tutorials build with podman on a golem-managed box. Both served
with a general-purpose web server that nothing chose: `1d00e89` added the
Containerfile to get the tutorial's site onto a box, and `mkWebsiteContainer`
copied that shape into nix when the image became a published artifact. No record
weighs it.

`dull-ca/nix` builds `nginx-static-no-tls` for exactly this job, and
`dull-ca/dull.yyc.dev` already serves its Starlight site with it. golem already
depends on that flake — `buildBunPackage` comes from its overlay — so the
package costs an attribute reference.

Two measurements separate the two servers.

**Closure.** The incumbent is 89,262,752 bytes; `nginx-static-no-tls` is
1,686,352 — a 53× difference, on an image whose payload is a 2.6 MB static site.

**What each one can be made to do.** The incumbent's TLS is off by configuration.
It ships an ACME client, a certificate cache and a TLS stack, and a one-line
config edit or a stray `--config` puts a public-facing HTTPS listener and a
cert-fetching robot on a box that is supposed to be behind Traefik.
`nginx-static-no-tls` is compiled without `ngx_http_ssl_module`; an `ssl`
listener is a config error, and dull-nix has a check asserting nginx rejects one.

The images sit behind Traefik and must never be reached directly. For that
placement, incapable beats configured-not-to.

## Decision

**Serve both images with nginx, configured by one `sites/website/nginx.conf`.**
The published image runs `dull-nix`'s `nginx-static-no-tls`; the tutorial image
runs `docker.io/library/nginx:alpine`, which is what a podman build on a Debian
guest can reach. Both read the same file, unmodified. The previous server leaves
the repository entirely.

**Run as `nobody` on `:8080`.** An unprivileged user cannot bind `:80` without
`CAP_NET_BIND_SERVICE`, and a privileged port buys nothing behind a proxy. Two
consequences of an empty base image follow from it: `dockerTools.fakeNss`
supplies the `/etc/passwd` entry that lets `nobody` resolve, and `extraCommands`
creates a 1777 `/tmp` for nginx's pid and client-body files — a `chmod` in the
`runCommand` would be reverted by nix's store canonicalisation, which resets
directory modes to 0555.

**Where the two builds differ, the difference lives in the image, not the
config.** `nginx:alpine` compiles its client-body temp path to
`/var/cache/nginx` and ships it root-owned, and its default command passes
`-g "daemon off;"` against a config that already sets `daemon off`. The
Containerfile chowns the directory and sets an explicit entrypoint. Neither
needs a directive in `nginx.conf`, so a reader has one file to edit and no
conditionals in it.

**`website-serves` asserts the behaviour, not the config.** It runs the real
nginx against the shipped `nginx.conf` and the real built site and checks four
things a reader would notice breaking: `/` answers 200 with `text/html;
charset=utf-8`, a missing path answers **404 *and* Starlight's styled
`404.html`**, a hashed stylesheet arrives gzipped as `text/css; charset=utf-8`,
and a directory URL without its trailing slash 301s to the canonical one.

## Consequences

- The published image drops from 31.6 MB to 1.5 MB compressed (9.6 MB
  uncompressed on a docker host). Every pull, every layer push, every cachix
  round trip pays that difference.
- No ACME client, no TLS stack, and no admin API reach either box. The failure
  mode where a misconfigured docs container starts soliciting certificates is
  gone by construction rather than by a config line someone must not edit.
- **One config, so a behaviour change lands in both images at once** — and
  `website-serves` covers only the published one. The tutorial image's base can
  drift under it: a future `nginx:alpine` that drops `ngx_http_gzip_static_module`
  or moves another compiled-in path fails at the Containerfile, not in
  `nix flake check`.
- **`zstd` encoding is lost.** The previous config asked for gzip and zstd; this
  nginx has no zstd module. gzip is what every client in practice negotiates,
  and the compressed sizes are within a few percent.
- Content types are what `nginx.conf` says they are. nginx's default
  `charset_types` exclude `text/css`, so the file names them; its `mime.types`
  calls JavaScript `application/javascript`, and that is left standing.
- The trailing-slash 301 needs `absolute_redirect off`. nginx builds that
  redirect's `Location` from `listen`, not from the request's Host, so behind
  Traefik an absolute one would point a reader at `:8080` on the public
  hostname. Relative Locations resolve against what the client actually asked
  for.
- `examples/website/website.emet` publishes `80:8080` rather than `80:80`. The
  host port and the firewall drop-in are unchanged; only the container side
  moved.
- **Conditional-request handling is untouched and still wrong.** Every file in
  the published image carries nix's epoch mtime, so `Last-Modified` never
  advances and a client revalidating with `If-Modified-Since` can be told `304`
  after a release that changed the bytes. The previous server had the identical
  defect, so this is not a regression — `dull.yyc.dev`'s `etag off` /
  `if_modified_since off` pair is the known fix, and adopting it is a separate
  decision about caching.
- golem and `dull.yyc.dev` now serve their sites the same way. A fix to the
  nginx build lands once, in `dull-ca/nix`, and both sites' checks catch a
  regression in it.
