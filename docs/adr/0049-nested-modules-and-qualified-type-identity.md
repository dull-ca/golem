# 0049 — Nested modules, and type identity that survives them

## Status

Accepted 2026-08-05. Extends ADR 0016 (the module system) and ADR 0024 (the
library search path). Closes ADR 0045's deferred option (b) and keeps its
rejection rule. Bears on ADR 0006 (dotted-name resolution), ADR 0046
(constructor collisions), and ADR 0048 (flat library resolution).

## Context

A module is one file, and a file is one module: `import Traefik` searches for
`Traefik.emet`, and a loaded module takes its name from the file stem. There is
no way to spell a module name with a dot — `import Limesurvey.Database` is a
parse error at the `.` — and no way for a directory to mean anything.

`examples/limesurvey/Limesurvey.emet` is what this costs. It holds three types,
two workloads, a route and a units list, and the only way to split it is
sibling files with flattened names: `LimesurveyDatabase.emet` beside
`Limesurvey.emet`, each name restating the prefix that a directory would have
carried.

The second force is the one that makes the first insufficient on its own.
**A type is known by its bare name across the whole program.** ADR 0045 found
that two modules exposing a type called `Thing` unified — a function typed
`A.Thing -> …` accepted a `B.Thing` — and fixed it by rejecting the collision:
one owner per type name, program-wide. That ADR weighed the alternative and
recorded its own verdict:

> (b) remains the better end state and is not superseded, only deferred

Splitting a module is exactly the operation that provokes the collision. Divide
`Limesurvey` by concern and both halves want a type named `Config`; the bare-name
rule rejects that program and forces `DatabaseConfig` and `SurveyConfig` — the
stutter the split existed to remove. Nesting without qualified identity is a
feature that fights the type system on first use.

Emet also permits something Elm does not: a module may expose a name it did not
declare. `module B exposing (thing)` with `import A exposing (thing)` compiles,
and an importer of `B` reads `A`'s value through it. Nothing in `lib/`,
`examples/`, `apps/fleet/`, or `sites/website/examples/` does this.

## Decision

**A module name may be dotted, and a dot is a directory separator.** `module
Limesurvey.Database` lives at `Limesurvey/Database.emet`, found by joining the
segments onto each ADR 0024 search-path root. A loaded module takes its name
from its path *relative to the root it was found under*, so the file location
and the module name cannot disagree. This is Elm's rule, adopted whole.

**Nesting confers no privilege.** `Limesurvey.Database` is an ordinary module
that happens to have a dot in its name. It gets no access to `Limesurvey`'s
internals, `Limesurvey` gets none to its, and neither is implied by the other's
existence — a parent module need not exist at all. Emet has one visibility
mechanism, the `exposing` list, and this ADR adds none.

**Type identity becomes module-qualified.** A type declared in module `M` is
identified as `M.Name`, so `Limesurvey.Database.Config` and
`Limesurvey.Survey.Config` are distinct types and both may be in scope.
Identity stays a `String`, qualified at the interface boundary, so the `Con(`
sites are untouched. Annotations continue to be written bare: an alias map
resolves a bare name in source to the qualified identity its scope names.

**ADR 0045's rejection rule stays, with a smaller job.** It no longer prevents
unsoundness — the types are distinct now — but the alias map is ambiguous
exactly when two imports contribute the same bare type name, and a program
cannot say which one an annotation means. That ambiguity is still rejected, with
the same diagnostic. What changes is that the rejection is now escapable: the
author can qualify the annotation rather than rename a type.

**A type may be named by its qualified spelling in type position.** ADR 0045
recorded "there is no qualified type syntax to escape into"; this ADR adds it.
`Limesurvey.Database.Config` in an annotation resolves to that module's type
whatever else is in scope.

**Diagnostics and hover render the bare name** wherever it is unambiguous in the
scope being reported, and the qualified name where it is not. A reader should
not pay for the qualification machinery in every type they read.

**`exposing` is restricted to locally declared names**, matching Elm: exposing a
name a module did not declare becomes an error naming it. This removes a
capability rather than declining to add one, which is why it is stated as its
own decision — it is safe today because nothing in the repo uses it, and it
keeps one module from silently becoming another's public surface.

**Qualified value access resolves the longest module prefix.** `A.B.c` is
member `c` of module `A.B` when that module is in scope, and otherwise falls
back to ADR 0006's dotted-name resolution, under which `List.map` is a single
identifier rather than module access. Prelude names are not modules and cannot
be shadowed by one.

**Library distribution is unaffected.** ADR 0048 resolves each library name to
one version globally; a library's submodules live under its own name and are
part of it. `Quadlet.Network` is not a separate resolution unit.

## Implementation notes

Both phases shipped: dotted names in `6f81a61`, qualified identity after it.

There is **no conversion funnel**. `ast.rs` declares the only `Type`, and
inference uses it directly, so there is no single point where source syntax
becomes an internal type and could be qualified in passing. The cheapest shape
found is a **pre-pass over the module AST**, run before inference at all three
entry points (`analyze_module`, `check_library`, `check_entry`): rewrite every
`Type::Con` in a `TypeDecl`'s variant fields and in every `Decl` signature from
its bare name to its qualified identity, leaving `TypeDecl::name` bare so the
`exposing` list still matches on the name the author wrote. `register_type_decls`
then needs only the owning module name, to build `result_ty` qualified and to
register the arity under that identity.

The alias map is bare name → qualified identity, built from the module's own
declarations plus everything its imports contribute. Where a bare name has two
candidates it maps to both, and *referencing* it is the error ADR 0045 used to
raise at import time.

`reject_type_name_collisions` and its two message builders are deleted:
identity now carries what the rule used to guard. `Interface` instead carries
`exposed_type_identity`, the bare name each exposed type is written as mapped to
the identity it holds — the `exposing` list, `import_type_arities` and the
constructor-visibility check all read it.

Every surface that shows a type to a reader de-qualifies: `render_type`,
`ast::Type`'s `Display` (hover and document symbols read it), and the messages
that name a constructor's type. `render_types_shared` is the exception — when
two identities in one message share a bare tail it prints both in full, which is
the case qualification exists for.

## Consequences

- Splitting a large module stops requiring flattened names. The
  `Limesurvey.emet` split is the first user and the proof.
- **Two type names that collide are no longer a program-wide error** — only an
  error where both are in scope and an annotation is bare. That readmits the
  programs ADR 0045 rejected, which was the point of deferring rather than
  dismissing (b).
- The qualification machinery is the cost: a type-substitution pass over every
  harvested interface, an alias map threaded through `register_type_decls`,
  `validate_type_refs` and `validate_signature_refs`, and a de-qualifying
  policy for `render_type`, diagnostics and LSP hover. ADR 0045 sized this as
  "a project rather than a session" and that estimate stands.
- **Constructors remain keyed by bare name** (ADR 0046), so qualified type
  identity does not by itself fix the recorded defect where two imports expose
  the same constructor name for differently-named types. It becomes reachable
  to fix — the owning type is now distinguishable — but that is a separate
  change. *Made in ADR 0051, which reuses this record's pre-pass, alias map and
  de-qualifying policy in the constructor namespace.*
- A module's name now depends on which search-path root it was found under, so
  the same file reached through two roots is two module names. The resolver
  must record the root it matched, not only the path.
- Removing re-export is a breaking change for any program outside this repo
  that used it. It is a language rule, not a wire format, so nothing at rest is
  affected.
- Deeper nesting invites deeper hierarchies for their own sake. Elm's ecosystem
  suggests two levels covers nearly everything; nothing here enforces a depth
  limit, and none is proposed.
- **Not adopted: a privileged parent–child relationship.** Rust's `mod` tree,
  where a child sees its parent's private items, is a different visibility
  model layered on a different unit of compilation. Adopting it would mean
  designing Emet's second visibility mechanism, and the `exposing` list already
  answers the question this feature raises.
