# 0048 — Library distribution: flat resolution, nix now, OCI later

## Status

Proposed 2026-08-02. Extends ADR 0024 (the library search path) and is
constrained by ADR 0045 and ADR 0046 (one type name and one constructor name
per module). Bears on ADR 0016 (the module system).

## Context

golem has its first real consumer outside this repo: `dulliac`. It takes
golem as a nix flake input, reads `${inputs.golem}/lib`, and symlinks that
directory into its own tree so `emetc` finds `Quadlet`, `Nftables` and
`Traefik` on the ADR 0024 search path. It works, it is hash-locked through
`devenv.lock`, and it cost no new tooling.

Its limits are real but not yet painful: only nix consumers can do it; the
granularity is the whole repository, so a library is versioned with golem
itself; and a third party cannot publish an Emet library at all.

The obvious next step is a package mechanism, and the obvious model is what
KCL does — publish modules as OCI artifacts to a registry. Registries are
ubiquitous, authentication and mirroring are solved, and a digest is
content-addressed, which is the same grain golem already uses for manifests.
golem also runs a registry in its own examples, so it can host its own
libraries with itself.

But the decision that actually constrains this is not the transport.

**Emet imports resolve by bare module name**, and a module's types and
constructors are nominal by bare name across a whole program. ADR 0045 fixed
a soundness hole where two imported modules exposing a type of the same name
unified; ADR 0046 closed the constructor half. The consequence for packaging
is absolute: **two versions of one library cannot coexist in one program.**
`Quadlet@1` and `Quadlet@2` are the same module name, and their types would
collide the moment either is exposed. No transport changes that.

## Decision

**Resolution is flat.** A program resolves each library name to exactly one
version, globally. A conflict between two requirements is an error naming
both requirers, not a nested install. This is Go's shape, not npm's, and it
follows from nominal bare-name identity rather than from taste.

**The dependency contract is separated from the transport.** A project
declares, in `emet.json`, each library it needs by name and content hash;
the resolver's job is to produce a flat name → hash map and a directory of
bytes for the ADR 0024 search path. Where the bytes come from is a transport
concern behind that contract.

**nix is transport #1, and is what ships now.** The flake-input-and-symlink
arrangement `dulliac` already uses is a conforming implementation: the flake
lock supplies the pin, the store path supplies the bytes. Formalising the
contract around what already works costs nothing and makes the alternative
additive.

**OCI is the intended transport #2**, when a non-nix consumer or a
third-party library exists. It is preferred over git-plus-lockfile because
registries already solve auth, mirroring and immutability, and over a bespoke
index because golem should not run a package registry.

## Consequences

- The hard decision — flat, one-version-per-name resolution — is made once,
  now, while there is a single consumer to migrate. Discovering it after a
  dependency graph exists would be far more expensive.
- A library cannot be upgraded for one consumer within a program without
  upgrading it for all of them. That is the honest cost of nominal bare-name
  types, and it is a reason to keep libraries small and their exposed
  surfaces narrow.
- Nothing about `dulliac` changes today. Its current arrangement becomes a
  named, specified thing rather than an accident, which is the point.
- Adding OCI later is additive: a new transport behind the same contract, no
  change to any consumer's Emet source, and no change to `emetc`'s search
  path.
- A content hash in `emet.json` gives an integrity check the symlink
  arrangement does not have today. Whether that hash is checked eagerly, and
  what `emetc` does when it disagrees, is left to the implementing ADR.
- Versioning semantics are deliberately unspecified here. Whether a
  requirement is an exact hash, a semver range, or a minimum version is a
  separate decision; this ADR only fixes that the *result* is flat.
- Publishing is out of scope. This says how a library is consumed, not how
  one is built, signed, or pushed.
