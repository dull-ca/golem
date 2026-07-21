# 0002-split-service-into-apt-package-and-systemd-service

## Status

Accepted. **The permissive *symmetric* injection described below was
superseded by ADR 0017:** it is now one-way widening (concrete glyph → `Glyph`,
never the reverse), and glyphs are matchable. The type-system note below is the
original interim mechanism, kept as the record; read ADR 0017 for what shipped.
The framing that drove the change: matching a glyph was never the hazard — the
*symmetric injection under elimination* was, and directed widening removes it.

## Context

The language's sole output is a glyph IR (`src/ir.rs`) — OS resources like apt
packages and systemd units. Until now there
was one primitive, `Resource::ServiceInstalled { package, unit }`, produced by
the reserved lowercase word `service { package = Str, unit = Str }`.

That single primitive conflated two distinct bottom-level OS actions:
installing a package and enabling/starting a systemd unit. The user wants those
modeled as two separate bottom-level primitives, and wants them usable as
first-class **types** in signatures (e.g. `webserver : Str -> SystemdService`).

## Decision

Split `service` into two primitives; `Glyph` becomes their sum.

- **IR (`src/ir.rs`).** Replace `ServiceInstalled` with two variants —
  `Glyph::AptPackage { name: String }` and
  `Glyph::SystemdService { unit: String }`.
- **Surface types (`src/ast.rs` `Type`).** Add `AptPackage` and
  `SystemdService` as first-class types, usable in signatures.
- **Two reserved lowercase record constructors:** `aptPackage { name = Str }`
  : `AptPackage`, and `systemdService { unit = Str }` : `SystemdService`. Both
  words are reserved (excluded from being ordinary variables), exactly as
  `service` was. The record form (not a bare string argument) is deliberate:
  these bottoms will grow more fields (version, source, enabled, …).
- **`Glyph` is the sum of the two:** conceptually
  `Glyph = AptPackage | SystemdService`, and `Glyphs = [Glyph]`. The two
  concrete glyph types **inject** into `Glyph`, so a mixed list still has
  type `Glyphs`, `main : Glyphs` keeps working, and existing `… -> Glyph`
  signatures remain valid.
- **Naming.** The sum type is `Glyph` (the list `Glyphs`): in the golem legend
  the creature is animated by the glyphs — inscribed marks — written on it. Each
  bottom-level primitive is one `Glyph`; the full `Glyphs` list is the complete
  desired state that animates Golem.
- **The old `service` primitive is removed** (decomposed into the two). A
  `service`-style convenience that emits both a package and a unit can live in
  userland as an ordinary function; it is no longer a bottom-level primitive.

### Type-system note

The sum is realised pragmatically in Algorithm W (`infer.rs`) by making
unification permissive: each concrete glyph type (`AptPackage`,
`SystemdService`) unifies with `Glyph` (the injection), while `AptPackage`
and `SystemdService` do **not** unify with each other. Because glyphs are
inert IR data with **no case-analysis / elimination form** in the language,
there is no operation that inspects "which glyph this is", so nothing can
misuse a subsumed value — this keeps the permissive rule sound in practice. If
glyph case-analysis is ever added, this must be revisited with proper
variant/row typing.

### Alternatives considered

1. **Keep the single `service` primitive.** Rejected: conflates two
   independent OS actions; can't type `-> SystemdService`.
2. **No umbrella type; make lists homogeneous** and restructure `main` into a
   record of typed lists
   (`{ packages : [AptPackage], services : [SystemdService] }`). Rejected by
   the user in favour of the sum: more disruptive to the surface language.
3. **Bare-string constructors** (`aptPackage "nginx"`). Rejected in favour of
   the record form for future field growth.

## Consequences

- The demo, `examples/*.emet`, and the pipeline + diagnostics test suites are
  updated to the two constructors; `key()` gains two namespaces (`apt:<name>`,
  `systemd:<unit>`).
- Adding a third primitive is now a well-trodden path: new IR variant +
  surface `Type` + reserved record constructor + inject into `Glyph`. The
  *language* machinery is unchanged, exactly as the project invariant intends.
- The permissive-unification injection is a standing soundness caveat: it holds
  only while glyphs have no elimination form (see the type-system note).
- Cross-references ADR 0001 (the parser/layout work) only incidentally.
