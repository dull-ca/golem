# 0045 — One type name, one owner, per module

## Status

Accepted 2026-08-01. Constrains ADR 0016 (the module system) and closes the
`docs/TODO.md` backlog-A KNOWN BUG "same-named types from two imports collide
in the constructor registry". Bears on ADR 0005 (exhaustiveness) and ADR 0044
(the argument-pattern gate), whose panics were the reported symptom but not the
disease. Touches no wire format: this is an `emetc` front-end rule, no
`format_version` change.

## Context

Emet identifies a type by its **bare name**. `Type::Con` (`ast.rs`) carries a
`String` and nothing else, so two `type Thing` declarations in two different
modules produce the same `Con("Thing", [])` and unify freely. Nothing in the
resolve stage ever noticed.

The bug was reported as a constructor-registry collision:
`resolve::import_constructors` fills `ctor_schemes` and `sum_ctors` keyed by
bare name, inserting per import in order, so `sum_ctors` keeps only the **last**
import's variant list while `ctor_schemes` accumulates constructors from every
import. A multi-constructor type therefore looks single-constructor, and the two
checks that read `sum_ctors` are corrupted: `case` exhaustiveness accepts a
non-exhaustive match (panicking at `eval.rs`'s
`unreachable!("non-exhaustive case")`), and ADR 0044's `reject_refutable_param`
admits a refutable parameter (panicking at
`unreachable!("refutable argument pattern survived inference")`). The `case`
route panics identically on a compiler built from `6a61b86`, so argument
patterns added a second door to one room rather than a new hole.

**Reproduction established that the merge is the lesser half of the problem.**
Four further programs, all of which type-checked and then failed at runtime:

- A function crosses the boundary with no `case` in the importer at all. Module
  `A` declares `type Thing = MkA` and `describeA : Thing -> String`, whose
  `case` is exhaustive *within A*. Module `B` declares `type Thing = MkB`. The
  importer writes `describeA MkB` — accepted, because both are `Con("Thing")` —
  and panics in A's own `case`. No corrupted `sum_ctors` is involved.
- The confusion carries a **value of the wrong type** into a slot the compiler
  proved. With `IntBox.Thing = Wrap Int` (`unwrap : Thing -> Int`) and
  `StrBox.Thing = Wrap String`, `unwrap (Wrap "not-an-int")` type-checks as
  `Int` and reaches the prelude as a `String` —
  `unreachable!("expected Int")`. A statically rejected program ran, and the
  runtime caught what the type system should have.
- Neither module need expose its type. With `Thing` private to both `OpaqueA`
  and `OpaqueB`, exposed only *inside* the signatures of exposed functions,
  `describe (make 5)` still type-checks and still fails. No `(..)` is written
  anywhere, and the importer cannot even name the type.
- One import suffices. A module declaring its own `type Thing` while importing
  a module whose exposed signatures mention a private `Thing` of its own is
  equally confused.

So the deeper unsoundness is **real, and independent of the constructor
registry**: a function typed `A.Thing -> …` does accept a `B.Thing`. The
exhaustiveness and argument-pattern panics are downstream reports of a broken
type identity, not the fault itself. Fixing only `import_constructors` — by
keying it per owning module — would silence two panics and leave the wrong-value
acceptance in place.

Two fixes were weighed.

**(b) Module-qualify type identity**, so `A.Thing` and `B.Thing` are genuinely
distinct. Correct long-term, and it would keep programs legal that this ADR
rejects. It does not require rewriting the ~36 `Con(` sites — identity can stay
a `String` that merely becomes qualified at the interface boundary — but it does
require: a type-substitution pass over every harvested interface; a bare →
qualified alias map threaded through annotation elaboration
(`register_type_decls`, `validate_type_refs`, `validate_signature_refs`); a
de-qualifying policy for `render_type`, diagnostics, and LSP hover, with the
corpus churn that implies; **and, still, a rejection rule** — the alias map is
ambiguous exactly when two imports expose the same bare type name, and
`ctor_schemes` remains keyed by bare constructor name regardless. (b) is
therefore a superset of (a), not an alternative to it, and a project rather
than a session.

**(a) Reject the collision.** Make the ambiguous state unrepresentable. Nothing
in `lib/`, `examples/`, `apps/fleet/`, or `sites/website/examples/` imports two
modules that share a type name, so the cost today is zero.

## Decision

**Within any one module, a type name has exactly one owner.**

Each module's harvested `Interface` carries `type_owners`: every type name
appearing in its **exposed surface** — the exposed type names, plus every
`Type::Con` head in the schemes of its exposed values and constructors — mapped
to the module that *declared* it. A name a module inherits through its own
imports keeps its original owner, so a type reached by two paths is one type.

Before inference, `resolve::reject_type_name_collisions` rejects a module when

- two of its imports contribute the same type name under **different** owners, or
- one of its own `type` declarations shares a name with an imported one.

The diagnostic names the type, names both defining modules, notes when a module
merely re-exposes a type it did not define, explains that Emet knows a type only
by its bare name, and says what to do: rename one of the two types, or import
the two modules from separate modules (ADR 0032).

The rule is scoped to the **exposed surface**, not to every declaration. A type
a module declares and never mentions in an exposed signature cannot reach an
importer, so two modules may both hold a private `Hidden` and still be imported
together.

Because a colliding name can no longer reach one module, `import_constructors`'
bare-name keys are unambiguous and stay as they are.

## Consequences

**Closed.** The wrong-value acceptance — the actual unsoundness — along with
both panics that reported it, in all shapes reproduced above: two open-exposed
types, two opaque ones, a local type against an imported one, and a plain
`import` with no `exposing` list. The check runs in the compile path and the
LSP analysis path, so the editor reports it too.

**Preserved.** A type reached through two imports is one type, not a collision:
`SharedLeft` and `SharedRight` may both re-expose `SharedType.Tag`, and
`examples/limesurvey/Ingress.emet` may keep importing `Traefik` twice. Every
`.emet` in the repo still compiles to a **byte-identical** manifest — verified
by building each of the 29 entries from this tree and from a `git archive HEAD`
extraction and comparing bytes, with the 10 non-entry modules compared by
diagnostic instead.

**Foreclosed.** A module may no longer import two modules that define the same
type name — even privately, when that type appears in an exposed signature —
and may no longer declare a type whose name an import already contributes. The
author must rename one of the two, or split the imports across two modules.
There is no qualified type syntax to escape into: Emet has no `A.Thing` in type
position, which is precisely why the rule has to be a rejection.

**Still open.** Two imports may expose the same *constructor* name for two
differently-named types (`CtorA.Alpha = Wrap String`, `CtorB.Beta = Wrap Int`).
`ctor_schemes` is keyed by bare constructor name and the last import wins, so
the earlier `Wrap` becomes silently unreachable and using it reports a type
mismatch against the wrong type. This is a **diagnostic** defect, not
unsoundness — the two types stay distinct, so no value crosses — and Elm rejects
the ambiguity outright. Recorded in `docs/TODO.md`.

(b) remains the better end state and is not superseded, only deferred: it would
readmit the programs this ADR rejects, at the cost of the qualification
machinery and the rendering policy described above. It would still need this
ADR's rejection rule for names ambiguous in scope.
