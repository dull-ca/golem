# 0046 — One constructor name, one owner, per module

## Status

Accepted 2026-08-02. Closes the item ADR 0045 left "Still open" and the
`docs/TODO.md` backlog-A KNOWN BUG "two imports may expose the same constructor
name". Constrains ADR 0016 (the module system) alongside ADR 0045, which it
neither supersedes nor amends: 0045 rules the type namespace, this one rules the
constructor namespace, and the two rules differ in scope and in what they
protect. Touches no wire format: an `emetc` front-end rule, no `format_version`
change.

## Context

A constructor is reachable **only by its bare name**. `CtorA.Wrap` is not a
type error or an unknown name — it is a *parse error*, `found '.' expected an
expression`. Emet has no qualified constructor spelling, in expressions or in
patterns.

`resolve::import_constructors` keys `ctor_schemes` by that bare name and inserts
per import in source order, so the last import wins. `resolve::import_ty_env`
binds `iface.exposed_constructors` bare and in the same order, with the same
last-wins result. With `CtorA` exposing `type Alpha = Wrap String` and `CtorB`
exposing `type Beta = Wrap Int` imported together, three programs were
reproduced against the compiler built from `326075f`:

- `a : Alpha` / `a = Wrap "text"` reports `type mismatch: expected Int, found
  String`. The annotation says `Alpha`, the error names `Beta`'s field type, and
  `Alpha` is never mentioned.
- `describe : Alpha -> String` whose body is `case v of Wrap s -> s` reports
  `type mismatch: expected Beta, found Alpha` against the *signature line*. The
  pattern side shadows identically to the value side.
- A module declaring its own `type Local = Wrap Int` while importing
  `CtorA exposing (Alpha(..))` compiles, silently, with the local `Wrap` winning
  and the imported one gone; writing the imported one reports the local type's
  field type.

This is **not unsoundness**. ADR 0045 keeps `Alpha` and `Beta` distinct, so no
value of one reaches code proved to hold the other; every program above is
rejected, just for the wrong reason. What it costs an author is a constructor
that disappears without a word and a diagnostic that indicts correct code by
naming a type the author never wrote.

Two fixes were weighed.

**Key `ctor_schemes` by owning module.** This is the fix that works for a
*type* name in a language with qualified type syntax — and Emet has none for
constructors either. Disambiguating the map does not help, because there is no
spelling at the use site to select an entry with: every occurrence of `Wrap` in
every expression and every pattern carries exactly one piece of information, the
bare name. Keying by owner would leave both entries reachable by nothing. The
only way to make a shadowed constructor usable is to invent
`CtorA.Wrap` — new grammar in expression *and* pattern position, a new
resolution path through `infer_pattern`, and a rendering policy for
diagnostics. That is a language feature, not a bug fix, and Elm — which Emet
follows here (ADR 0016) — does not have it: Elm rejects the ambiguous import
instead.

**Reject the collision.** Follows from the reachability answer rather than from
symmetry with ADR 0045: since a same-named constructor from a second module can
never be *named*, admitting it buys the author nothing and costs a vanished
constructor. Rejection also matches a rule the compiler already enforces one
scope down — `infer::register_type_decls` refuses a `type` declaration whose
variant name duplicates another local variant (`duplicate constructor \`W\``) or
a prelude constructor. So "one constructor name, one owner" is not new policy;
it is the existing rule, which stops at the module boundary, carried across it.

Nothing in `lib/`, `examples/`, `apps/fleet/`, `apps/emet/examples/`, or
`sites/website/examples/` collides, so the cost today is zero.

## Decision

**Within any one module, a constructor name has exactly one owner.**

Each module's harvested `Interface` carries `ctor_owners`: every constructor its
exposed surface contributes, mapped to a `ConstructorOrigin` — the module that
declared it and the type it builds. Before inference,
`resolve::reject_constructor_name_collisions` rejects a module when

- two of its imports contribute the same constructor name under **different**
  owners, or
- one of its own `type` declarations has a variant whose name an import already
  contributes.

The diagnostic names the constructor, names both modules, names the type each
one builds, states that Emet reaches a constructor only by its bare name and has
no qualified spelling, and says what to do: rename one of the two constructors,
or import the two modules from separate modules (ADR 0032).

Two properties make the rule narrower than ADR 0045's, and both follow from
constructors being gated more tightly than types:

- **The exposing list is the whole scope.** `interface_of` harvests constructors
  only for `Type(..)` open exports, so a constructor behind a closed export
  cannot be named or matched by an importer and cannot collide. ADR 0045 has to
  reach past the exposing list — a *private* type named in an exposed signature
  still unifies by name in the importer — but a private constructor is genuinely
  invisible.
- **Ownership never propagates.** `interface_of` reads constructors from the
  module's own `type_decls`, so no module can re-expose an imported
  constructor. `ConstructorOrigin::owner` is therefore always the exporting
  module itself, and the constructor check needs no analogue of
  `inherited_type_owners`. One module imported twice contributes one owner and
  passes.

`import_constructors` and `import_ty_env` keep their bare-name keys: after the
check, no module can reach two constructors under one name, so last-wins has
nothing to lose.

## Consequences

**Closed.** All three shapes above — value position, pattern position, and a
local declaration against an imported one — now report the collision itself,
at the second `import` line or at the offending variant, instead of a type
mismatch against the surviving constructor's type. The check runs in the compile
path and in the LSP analysis path, so the editor reports it too.

**Preserved.** Every `.emet` tracked in the repo — 150 files, 42 that build a
manifest and 108 modules and negative fixtures that report a diagnostic —
compiles to **byte-identical** output, verified by running the compiler from
this tree and from a `git archive HEAD` extraction over each file and comparing
stdout bytes, exit status, and rendered stderr. Importing one module twice
(`examples/limesurvey/Ingress.emet` imports `Traefik` twice) still compiles, and
so does a local constructor sharing a name with one an import declares but never
exposes.

**Foreclosed.** A module may no longer import two modules that open-expose the
same constructor name, nor declare a constructor whose name an import already
contributes. The author renames one of the two, or splits the imports across two
modules. There is no qualified constructor syntax to escape into — that absence
is the reason the rule rejects rather than disambiguates, and adding such syntax
later would be the thing that reopens this decision.

**Not addressed.** A constructor name colliding with a *prelude* constructor
across the module boundary cannot arise: `register_type_decls` already refuses
`type X = Just Int` in the module that would export it, so no interface can
carry one.
