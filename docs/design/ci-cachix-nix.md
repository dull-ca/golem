# CI with nix + cachix

How golem gets built, tested, and cached after the move off Codeberg. Decided in
principle by ADR 0035; this is the concrete, runnable sketch.

## The model

One gate, one cache, and any machine can be CI:

- **The gate is `nix flake check`** on the repo flake. It runs the whole
  workspace test suite, the `apps/fleet` harness, and the four binary builds
  (`flake.nix` `checks`). Nothing about CI is a separate system — a dev machine
  runs the identical command and gets the identical result.
- **The cache is one cachix cache.** The CI box builds the flake outputs and
  pushes their store paths; every other machine substitutes from the cache
  instead of rebuilding golem's crates and its nixpkgs closure.
- **The CI box is just the machine that runs the gate on every push** and pushes
  the closure to cachix. It is provisioned by golem itself (dogfood) — see the
  loop below.

The cache does not exist yet (Dr. Dub is creating it), so `<cache>` is a
placeholder throughout. Nothing here is wired into `flake.nix` until the real
name and public key exist.

## One-time cachix setup (Dr. Dub)

Create the cache and hand its write token to the CI box; dev machines only need
read (public caches serve reads with no token).

```nu
# 1. Create the cache at https://app.cachix.org (web UI) — pick a name, note it as <cache>.

# 2. Generate a write auth token for the CI box (Cachix > <cache> > Settings > Auth Tokens),
#    or from a machine already logged in:
cachix authtoken --help   # generate/scope the write token; keep it for the CI box only

# 3. On each dev machine, trust the cache for reads (imperative):
cachix use <cache>
```

On NixOS, prefer wiring the cache declaratively over `cachix use` so it survives
a rebuild — add to the host's configuration (the public key is printed on the
cache's cachix page):

```nix
nix.settings = {
  substituters = [ "https://<cache>.cachix.org" ];
  trusted-public-keys = [ "<cache>.cachix.org-1:<public-key>" ];
};
```

## Flake follow-up (once the cache exists)

Add the cache to `flake.nix` so any `nix build` against the repo offers it
without per-machine setup:

```nix
nixConfig = {
  extra-substituters = [ "https://<cache>.cachix.org" ];
  extra-trusted-public-keys = [ "<cache>.cachix.org-1:<public-key>" ];
};
```

A consumer opts in with `--accept-flake-config` (or `accept-flake-config = true`
in their `nix.conf`).

**Not added yet, deliberately.** `nixConfig` with a placeholder or wrong cache
name makes nix print a trust warning on every command run against the flake. Add
this only when `<cache>` and `<public-key>` are the real values — this is the
"Flake follow-up" item in `docs/TODO.md` §C.

## The CI box loop (golem-managed — sketch)

golem provisions the CI box with its own four glyphs. **The box does not exist
yet; this is the shape, not a built fleet.**

- **`file` glyph — the poll-and-build script.** `git fetch` against GitHub
  `main`; on a new commit:
  1. `nix flake check` — the gate (§The model).
  2. `cachix watch-exec <cache> -- nix build <release outputs>` — build the
     release binaries and push every store path they pull in to the cache.
  3. the impure website step: `bun run build`, then
     `nix build --impure .#website-container`.
- **`systemdService` glyph — a timer/service pair** that runs the script.
  Polling is the first cut; a GitHub webhook trigger is a later refinement.
- **A `file` glyph — the cachix auth token**, placed at mode `0600`, owned by
  root or a dedicated `ci` user. The write token from the one-time setup lives
  here and nowhere in the repo. It has to be a `file` and not a `lineInFile`:
  a `lineInFile` owns one line and not the file it appends to, so it cannot
  promise a mode, and `emetc` refuses a secret written that way (ADR 0047).

golem provisioning the box that runs golem's CI is the dogfood ADR 0035 §2
names. Webhook-triggered rather than polled, and a dedicated `ci` user rather
than root, are refinements for when the box is actually stood up.

## What stays manual / impure

- ~~The website dist and container.~~ Both build purely now and both are
  `checks`: `websiteDist` via `buildBunPackage`, `website-container` on top of
  it. Nothing about the docs site is impure or manual.
- **Release publishing.** The mechanism is an open question (ADR 0035 §5): the
  Forgejo/Codeberg channel died with the move off Codeberg, and its replacement
  — GitHub Releases pushed from the box, or artifacts served from Dr. Dub's own
  infrastructure — is not yet decided.
