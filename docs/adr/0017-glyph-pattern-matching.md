# 0017-glyph-pattern-matching

## Status

Proposed (for review — **design only, not to be implemented from this ADR**).
Supersedes the deferral in ADR 0008 when accepted; would flip ADR 0008 to
"Superseded by 0017" and retire the interim symmetric-injection caveat on
ADR 0002.

## Context

Emet's four glyph primitives (`AptPackage`, `SystemdService`, `File`,
`LineInFile`), their sum `Glyph`, and the per-host container `Scroll` are
**constructible but not matchable**. A user can write `aptPackage { name = … }`
and build a `List Glyph`, but cannot `case` on a `Glyph` to ask which primitive
it is or read its fields. Every other sum in the language — user `type` decls,
the prelude's `Maybe`/`Bool`/`Order`, and even `List` (as the synthetic `[]`/`::`
sum) — is matchable with compile-time exhaustiveness (ADR 0005). Glyphs are the
one hole.

ADR 0008 deferred glyph matching and named the precise reason it was unsafe to
add casually: **not matching itself**, but ADR 0002's *symmetric permissive
injection*. `infer.rs::unify` has an arm (`glyph_injects`) that unifies each
concrete glyph type with `Glyph` **in both directions** without recording which
variant a `Glyph`-typed value is; `widen_glyph_subtype` promotes a concrete
glyph to `Glyph` at list/branch joins. Because the injection is symmetric, a
`Glyph`-typed hole can be satisfied by *any* concrete glyph with its identity
never pinned. An elimination form that asks "which glyph is this?" could then
inspect a value whose concrete identity was never established — unsound. ADR 0008
set a precondition: **replace the symmetric arm with a principled model before
adding any glyph elimination**, and recommended *directed nominal subsumption*
(concrete → `Glyph` only) over full row/polymorphic variants, since glyphs are a
closed, compiler-owned set, not a user-extensible row.

Since ADR 0008, three things have landed that change the cost calculus:

- **Exhaustiveness over sums, lists, and arity>0 types (ADR 0005 + addenda).**
  Maranget's usefulness algorithm already runs over any type whose complete
  constructor signature `sum_type_constructors` can report and whose per-variant
  argument types `constructor_scheme` can supply. `List` joined it as a synthetic
  two-constructor sum with **zero** list-specific code in the checker — only two
  synthetic constructors and their schemes. This is the template a glyph model
  reuses verbatim.
- **Row-polymorphic records (ADR 0010).** Records now carry an open/closed `Row`
  tail and unify structurally. ADR 0010's own consequence note flags this as
  "most of what a principled glyph pattern-matching model needs." It matters here
  because glyph fields are **named records**, not positional args (`aptPackage
  { name }`, `file { path, contents, mode }`), and record patterns are the
  natural way to bind them.
- **The Elm-shaped module system (ADR 0016).** An open-exposed (`Type(..)`)
  import already carries a type's full constructor set across module boundaries
  so an importer can match it exhaustively. Glyphs are compiler-owned (always in
  scope, never imported), so they need none of this — but it confirms the
  machinery treats "a sum's variants" uniformly regardless of origin.

**Elm is the muse.** Elm has *no subtyping*: `Glyph` in Elm would be a plain
nominal ADT, `type Glyph = AptPackage {…} | SystemdService {…} | File {…} |
LineInFile {…}`, and every value of it is *always* known to be one tagged
variant — there is no "concrete `AptPackage` that is also secretly a bare
`Glyph`." Matching such a sum is a trivial `case` and is trivially sound. The
whole hazard ADR 0008 describes is an artifact of Emet's *subtyping* shortcut,
which Elm does not have. So the Elm-faithful target is: `Glyph` behaves like an
ordinary nominal sum whose four variants are the primitives, matched like any
`type`.

Two frictions stand between today's code and that target:

1. **Constructor spelling asymmetry.** The four glyph constructors are reserved
   **lowercase** words (`aptPackage`, …) lexed as `Tok::Ident` and special-cased
   in `parser.rs::parse_atom` into dedicated `Expr::AptPackage`/`File`/… nodes —
   *not* `Expr::Ctor`. Every *matchable* constructor elsewhere is **PascalCase**
   (`Tok::Upper`, `Pattern::Ctor`). A user writing `case g of AptPackage p -> …`
   is naming a constructor that, on the construction side, is spelled
   `aptPackage`. The type is `AptPackage` (PascalCase) but the value constructor
   is `aptPackage` (camelCase). This split must be resolved.

2. **Value representation.** A glyph evaluates to `Value::Glyph(scroll_format::Glyph)`
   — a wrapped foreign enum — not to `Value::Data { ctor, args }` like every
   matchable constructor. `match_pattern`'s `Ctor` arm only matches `Value::Data`.

Neither friction is fundamental; both are consequences of glyphs having been
build-only. What breaks vs. what is additive is the crux of the decision below.

## Decision

**Make `Glyph` an ordinary user-facing nominal sum whose four variants are the
primitives, matched with the existing `case`/exhaustiveness machinery — via
record-field patterns for each variant's fields. Replace the symmetric injection
of ADR 0002 with directed subsumption so the sum is sound under elimination. No
wire-format change.** This is ADR 0008's route 2 (directed nominal subsumption),
now the cheaper of the two because the sum machinery it plugs into already
exists.

Concretely, five pieces — all language-side (`apps/emet/src/*`), none touching
`scroll-format`:

### 1. Register the glyph variants as a sum, exactly like `List`

Extend `prelude::sum_type_constructors("Glyph")` to report the four variants and
`prelude::constructor_scheme` to give each its scheme, mirroring how `NIL`/`CONS`
were added for `List`. The variant *tags* the checker sees are the four glyph
constructor names; each variant carries **one record argument** (its fields), so
the schemes are:

```
AptPackage      : { name : String }                      -> Glyph
SystemdService  : { unit : String }                      -> Glyph
File            : { path : String, contents : String, mode : String } -> Glyph
LineInFile      : { path : String, line : String }       -> Glyph
```

With these two functions extended, the Maranget checker treats `Glyph` as a
four-constructor sum with **no glyph-specific code in the checker** — precisely
the `List` precedent. `case g of AptPackage p -> … ; SystemdService s -> … ;
File f -> … ; LineInFile l -> …` is exhaustive exactly when all four tags are
covered (or a `_`/var arm covers the rest); a repeated or post-catch-all arm is
redundant. `builtin_type_arity`/`builtin_types` already list `Glyph` at arity 0,
so nothing there changes.

### 2. Patterns bind fields as records, not positional args

A glyph variant's payload is a **record**, so its pattern binds a record:
`AptPackage p` binds `p : { name : String }`; the field is then read with the
existing row-polymorphic `.name` (ADR 0010). This reuses `Pattern::Ctor(name,
subpats)` with a *single* sub-pattern whose inferred type is the variant's record
type — `infer_pattern` already instantiates a constructor scheme, unifies its
result with the scrutinee, and types sub-patterns against the argument types
(here, the one record type). Optionally, a later ergonomic pass can allow an
inline record pattern `AptPackage { name }` that binds `name` directly; that is a
**pattern-language addition** (a record pattern) orthogonal to this ADR and
deferred. The minimum is: bind the record, project with `.field`.

### 3. Resolve the constructor-spelling asymmetry: PascalCase tags in patterns

Patterns name the **type-cased** tags (`AptPackage`, `File`, …) — these are what
`Tok::Upper` + `Pattern::Ctor` already parse, and what reads naturally as "match
the `AptPackage` case." The camelCase construction words (`aptPackage`, …) stay
exactly as they are on the **build** side (ADR 0002's record-form constructors,
unchanged). So the language gains a deliberate, documented convention: the four
primitives are *built* with their lowercase reserved word and *matched* by their
PascalCase tag — the tag is the type name, which is how a reader already refers
to the variant. `parser.rs::is_reserved_constructor` and `build_constructor` are
untouched; matching goes through the ordinary `Tok::Upper` → `Pattern::Ctor`
path, with `infer_pattern`/`constructor_scheme` resolving the four tags to the
schemes from piece 1. (A symmetric future option — also allow *building* via the
PascalCase tag applied to a record — is possible but out of scope; this ADR only
adds the *match* direction.)

### 4. Replace the symmetric injection with directed subsumption

This is the soundness core and ADR 0008's stated precondition. Today
`glyph_injects` fires in `unify` **both directions** (`glyph_injects(n1,…,n2,…)
|| glyph_injects(n2,…,n1,…)`). Replace the *bidirectional* unify arm with a
**directed** rule: a concrete glyph type may be *widened* to `Glyph` (the
existing `widen_glyph_subtype`, already applied at list literals, `if`, and
`case`-arm joins), but `unify` no longer treats a bare `Glyph` as satisfiable by
an unwidened concrete glyph in the reverse direction. In practice the widening
already happens at every join that builds a `List Glyph`, so removing the reverse
arm of the symmetric injection is expected to be close to behaviour-preserving
for existing programs — but it is the change that makes "given a value typed
`Glyph`, ask which variant it is" sound, because a `Glyph`-typed scrutinee can
now only have arisen by an explicit widen, never by an un-pinned reverse
injection. The `NOTE` on the `unify` arm and the interim caveats on ADR 0002 /
ADR 0005 come down.

### 5. Eval: match `Value::Glyph` by reifying its tag and fields

`match_pattern`'s `Ctor` arm currently matches only `Value::Data`. Add an arm (or
a small adapter) that, for a `Value::Glyph(g)`, reads `g`'s variant tag and
fields and matches them against the pattern's tag and record sub-pattern:
`Glyph::AptPackage { name }` matches tag `AptPackage` binding a record value
`{ name }`, and so on. This is a read-only projection of the already-evaluated
foreign enum — no new `Value` case, no change to how glyphs are *built* or
serialized. The exhaustiveness guarantee means the no-match path stays
`unreachable!`, same as every other `case`.

### Scrolls fall out for free

A `Scroll` is `{ name : String, glyphs : List Glyph }`. Matching a scroll's
glyphs is **already** list patterns (ADR 0005 addendum) over `Glyph` matching
(this ADR): `case scroll.glyphs of [] -> … ; g :: rest -> case g of …`
type-checks and is exhaustive with no scroll-specific work — list patterns supply
the `[]`/`::` coverage, glyph matching supplies the four-tag coverage, and they
compose. `Scroll` itself need not become a matchable sum: it is a single-shape
record, and its one field is reached with `.glyphs`/`.name` via ADR 0010. If a
`case scroll of Scroll { … }` destructuring is ever wanted it is a trivial
one-variant addition, but it buys nothing over field access and is **not**
proposed here.

## Alternatives considered

1. **Polymorphic / row variants for `Glyph` (ADR 0008 route 1).** Model `Glyph`
   as an open/extensible variant row, constructors injecting with precise row
   types, `case` matching tags with row-based exhaustiveness. **Rejected as
   over-built:** glyphs are a *closed, compiler-owned* set of exactly four
   variants (adding a fifth is a `format_version` wire change, not a userland
   act), so the openness a variant row buys is unusable. It would add row-variant
   unification machinery distinct from the record rows of ADR 0010, whereas route
   2 reuses the existing nominal-sum checker unchanged. Reserve row variants for
   a hypothetical future of *user-defined* open variants — still not in scope.

2. **Keep glyphs opaque; expose accessors/predicates instead** (e.g. a builtin
   `Glyph.name : Glyph -> Maybe String`, or `isAptPackage`). **Rejected:** this
   is a non-Elm, non-exhaustive API — every accessor is a partial function
   forced to return `Maybe`, exhaustiveness is lost, and it grows a bespoke
   builtin per field forever. `case` is the language's one elimination form and
   glyphs should not be a second-class exception to it.

3. **Retain the symmetric injection and add matching anyway.** **Rejected:** this
   is exactly the unsoundness ADR 0008 identified — a `Glyph`-typed value whose
   concrete identity was never pinned, then interrogated by a `case`. Matching
   without first replacing the symmetric arm reintroduces the hazard the deferral
   existed to avoid.

4. **Make `Glyph` an actual user-space `type` decl in a prelude source file**
   (fully erasing the compiler special-case). **Rejected for now, but noted as
   the eventual clean form:** the four constructors are still reserved lowercase
   words wired to `scroll-format` variants and to `key()`/serialization, so they
   cannot yet be an ordinary `type Glyph = …` without also unifying the
   construction spelling and the value representation. This ADR takes the smaller
   step (register the sum in the prelude registry, keep the reserved
   constructors) and leaves "glyphs as an ordinary prelude `type`" as a possible
   later consolidation.

5. **Forbid glyph matching permanently.** **Rejected** (as in ADR 0008):
   unnecessary; matching is not the hazard and closing the door loses a
   plausibly-useful capability — inspecting/transforming a fleet's glyphs in
   Emet itself (e.g. a library that rewrites every `File` mode, or asserts no
   `LineInFile` targets a given path).

## Consequences

- **`Glyph` becomes a first-class matchable sum**, consistent with every other
  sum in the language; the last elimination-form hole closes. Users can write
  glyph-transforming and glyph-asserting libraries in Emet, exhaustively checked.
- **The ADR-0002 symmetric injection is removed**, retiring the standing
  soundness caveat carried on ADR 0002 and ADR 0005. `Glyph`-typed values are now
  sound under elimination because they can only arise by directed widening. This
  is the one arm that changes in `unify`; the risk to existing programs is the
  behaviour of removing the *reverse* injection direction, which existing
  `List Glyph` construction is expected not to rely on (widening already fires at
  the joins) — this must be verified against the pipeline suite before
  implementing.
- **A deliberate construct-vs-match spelling convention** enters the language:
  build with the lowercase reserved word (`aptPackage`), match by the PascalCase
  tag (`AptPackage`). This is a documentation and teaching cost; it is the price
  of not also reworking the four reserved constructors (alternative 4). The
  convention should be stated plainly in `sites/website` and the design doc.
- **Additive, not a repaint** — as ADR 0008 intended the foundation to allow. The
  Maranget checker, `Pattern::Ctor` inference, and row-polymorphic field access
  are reused as-is; the new code is: two prelude-registry extensions, one
  directed-subsumption edit in `unify`, and one `match_pattern` arm. No new
  `Value` variant, no new pattern kind (record-pattern sugar is deferred).
- **No wire / `format_version` implications.** This is entirely language-side.
  `scroll-format::Glyph`/`Scroll`, the postcard encoding, content-ids, `key()`,
  and golemd are untouched; a glyph value is only *read* during matching, never
  reshaped. The manifest a program compiles to is byte-identical whether or not
  the program used a glyph `case`.
- **Record-pattern sugar (`AptPackage { name }`) is left open** as a follow-up
  ergonomic; until then fields are reached by binding the record and projecting
  with `.field`. Inline one-line `case` remains deferred per ADR 0005.
- **Forecloses** treating `Glyph` as a subtype-bearing type: after this, `Glyph`
  is a plain nominal sum in the Elm sense, and the "concrete glyph is also a bare
  Glyph" subsumption intuition is gone (replaced by explicit widening). Any future
  feature must treat glyphs as ordinary tagged variants.

## Cross-references

- **ADR 0008** (glyph pattern-matching deferred) — this ADR is its resolution;
  adopts its recommended route 2 and satisfies its precondition. Flip 0008 to
  *Superseded by 0017* on acceptance.
- **ADR 0002** (glyph primitives; concrete-subtype / permissive injection) — the
  symmetric arm removed here; its interim caveat retires.
- **ADR 0005** (custom types, `case`, exhaustiveness; + list-pattern and
  arity>0 addenda) — the elimination + exhaustiveness machinery this reuses
  wholesale; its "glyphs stay non-matchable for now" note is lifted.
- **ADR 0010** (row-polymorphic records) — the record-field access that reads a
  matched glyph's fields; the machinery whose arrival made this the cheaper route.
- **ADR 0009** (scroll per-host container) — `Scroll` matching falls out of list
  patterns + glyph matching with no scroll-specific work.
- **ADR 0016** (Elm-modeled module system) — confirms the sum machinery treats a
  type's variant set uniformly; glyphs are compiler-owned so need no import path.
- **ADR 0012 / 0013** (binary content-addressed manifest; `scroll-format` crate)
  — unaffected; no `format_version` bump.
- Design doc `docs/design/0001-…` §5 (glyph subsumption vs. `List a`) — the
  interim symmetric-injection discussion this decision closes out.
