# Design: Elm-lite type system and value language for Emet

Status: **implemented through Wave 7** (this design is realized in the current
code; the `Scroll` output container of ADR 0009 supersedes the `main : List
Glyph` framing below).

Status: **Proposed** (design only; no implementation)

Author: architect pass, grounded in the code as of commit `2ba47b7`.

**Caveat (ADR 0011):** this design's totality claims — "no general/self
recursion", "guaranteed termination", "finite glyph DAG" — described the
language as designed at the time of writing. ADR 0011 later relaxed this:
Emet now permits general self-recursion, so termination is a soft
preference, not a guaranteed invariant. Exhaustiveness checking for `case`
is retained regardless. Read totality statements below as historical
context, not current fact.

Companion ADRs (drafts in `docs/adr/`):
- 0003 — generics: type variables, type application, `Type` representation.
- 0004 — no-templating IR principle + string interpolation.
- 0005 — `case … of`, custom types, exhaustiveness, and totality.
- 0006 — module-qualified built-ins (`List.` / `Maybe.` / `String.`).
- 0007 — numbers (`Int`/`Float`), constrained type variables (`number` /
  `comparable`), and infix operators with Elm precedence.
- 0008 (stub) — glyph pattern-matching: planned, deferred, foundation kept
  variant-ready.

Revision note: this revision (a) confirms `Str → String` (with a transitional
`Str` alias) as **decided**, not open; (b) brings **numbers fully in scope**,
following Elm — `Int`/`Float`, `number`/`comparable` constrained type variables,
and infix operators with Elm precedence — reversing the earlier "numbers /
infix are non-goals"; and (c) corrects the glyph framing (§5/§12): glyph
pattern-matching is *kept open and deferred*, the only interim unsoundness is
the ADR-0002 **symmetric permissive injection under elimination**, not matching
itself.

---

## 1. Goal and framing

Grow Emet from "records + lambdas + two glyph primitives" into a **fairly
complete but Elm-*lite*** typed functional language: enough of Elm's surface and
semantics to be genuinely usable and pleasant, deliberately not all of it. The
guiding rule is **exact Elm naming and semantics** wherever we implement a
feature — `Maybe a = Just a | Nothing`, `List a`, `Bool = True | False` with
`if … then … else …`, `case … of` pattern matching, module-qualified functions
(`List.map`, `Maybe.withDefault`, `String.concat`), and Elm-accurate signatures
(`map : (a -> b) -> List a -> List b`).

Numbers are **in scope and modeled after Elm faithfully**: `Int` and `Float`,
Elm's `number`/`comparable` constrained type variables, and infix operators with
Elm precedence. This is the one place Elm itself leaves pure Hindley-Milner
(bounded, typeclass-*like* constraints), and we follow it exactly that far and no
further — no user-defined typeclasses (§4.4, ADR 0007).

The single **intentional divergence** from Elm is **string interpolation**
(`"port ${port}"`), which Elm does not have. Everything else that diverges does
so only because of Emet's two hard invariants (totality; the IR is the sole,
inert output) and those divergences are enumerated in §12.

The good news, established by reading `src/infer.rs`: **the hard part is already
done.** Algorithm W — `generalize` / `instantiate` / `unify` / `ftv` / `occurs`
over `Type::Var` — already implements parametric polymorphism. `id x = x`
already generalizes to `∀t. t -> t` and is already used at two types in
`tests/pipeline.rs::hm_infers_polymorphic_identity`. The gap is **surface
syntax and type representation**, not the inference math. This design is mostly
about (a) letting signatures *mention* type variables and applied constructors,
(b) adding sum types + `case`, and (c) desugaring niceties (interpolation, `if`,
qualified built-ins) onto the existing core.

---

## 2. What exists today (ground truth)

Precise starting point, so every proposed change is a delta against real code.

**`src/ast.rs`**
- `Type = Var(u32) | Str | AptPackage | SystemdService | Glyph | Glyphs | Fun | Record`.
  `Var(u32)` is **only** a unification variable — there is no surface syntax that
  produces one. `Glyphs` is a bespoke "list of Glyph" type, not `List Glyph`.
- `Expr = Str | Var | AptPackage | SystemdService | List | Lam | App | Let | Record | Field`.
- `Scheme { vars: Vec<u32>, ty: Type }`; `Decl { name, sig: Option<Spanned<Type>>, params, body, span }`.

**`src/infer.rs`** (Algorithm W — already polymorphic)
- `unify` has permissive arms injecting `AptPackage`/`SystemdService` into
  `Glyph` (ADR 0002). `AptPackage` and `SystemdService` do **not** unify with
  each other. Sound only because glyphs have **no elimination form**.
- `infer_decls` infers each decl left-to-right, generalizes, then inserts it —
  **no mutual recursion, no self-recursion** (a decl is generalized and bound
  *after* its body is inferred, so its own name is not in scope in its body).
- **The signature-check line is the key constraint.** In `infer_decls`:
  ```rust
  if let Some(sig) = &decl.sig {
      // The signature uses concrete types only (no variables), so unify.
      inf.unify(&inferred, &sig.0, &sig.1)?;
  }
  ```
  A signature is a `Type` with **no `Var`s**, unified directly against the
  inferred body. The moment a signature says `a`, this is wrong: each `a` must
  become a *fresh unification var, consistently per name within the signature*,
  and then unified. (§4.3.)

**`src/parser.rs`** (chumsky 0.10)
- `type_parser` knows only the fixed `Upper` names (`Str`, `AptPackage`,
  `SystemdService`, `Glyph`, `Glyphs`), `[Glyph]` (only `[Glyph]`, hard-coded),
  record types, parens, `->`. No type vars, no type application.
- `expr_parser`: string literal, the two reserved constructors, `var`, parens,
  list, record, field access (postfix `.f`), application (juxtaposition),
  lambda, `let … in`. **No** `case`, `if`, infix operators, interpolation,
  qualified names, or bare constructors.
- `aptPackage`/`systemdService` are special-cased in `parse_atom` (excluded from
  `var`, parsed as record constructors).

**`src/lexer.rs`**
- String literals with escapes (`\n \t \" \\`, and `\x → x` for any other `x`).
- Keywords `let in where of`; `of`/`where` participate in layout but are unused.
- `Upper`-initial → `Tok::Upper`; lowercase/`_`-initial → `Tok::Ident`.
- No `${` handling; no `if/then/else/case/type` keywords.
- **No numeric literals at all** (no integer or float tokens), and **no operator
  tokens** beyond `-> = \ : , . ( ) [ ] { }`. Adding numbers means new literal
  tokens *and* an operator-symbol lexing path; both are green-field (§4.4, §9.5,
  ADR 0007).

**`src/layout.rs`**
- Offside rule; the **only** `parse-error(t)` trigger is close-on-`in` for a
  `Keyword`-origin block. ADR 0001 explicitly warns: adding `where`/`case … of`
  requires new close rules. `of`/`where` already open layout blocks in
  `opens_layout`.

**`src/eval.rs`**
- `Value = Str | Glyph | Glyphs | Record | Closure`. **No sum/constructor
  value.** `List` evaluation flattens `Glyph`/`Glyphs` elements into a
  `Vec<Glyph>` — it is *not* a general cons list; it is glyph-list-specific.

**`src/ir.rs`**
- `Glyph = AptPackage { name } | SystemdService { unit }`; `key()`,
  `describe()`. `lib.rs::analyze` dedups by `key()`, erroring on conflicting
  content for the same key.

**Invariants to preserve** (from CLAUDE.md + ADR 0002):
1. **Totality**: guaranteed termination, finite glyph DAG, no general recursion.
   *(Superseded by ADR 0011: self-recursion is now allowed; totality is a
   soft preference, not a guarantee.)*
2. **IR is the sole output**; language evaluates to `Vec<Glyph>`.
3. **Small dependency footprint** (`ariadne`, `chumsky` only).
4. **Glyph subsumption** keeps working; permissive unification is sound *only
   while glyphs have no elimination form.*

---

## 3. Design overview (the shape of the answer)

We make six moves, each a clean layer on the existing pipeline:

1. **Generalize the type representation** so `Type` can express type variables
   *and* applied type constructors: replace the ad-hoc `Str/Glyph/Glyphs` +
   future `List a`/`Maybe a`/`Bool` with a single `Type::Con(String, Vec<Type>)`
   node plus a rigid-var node for signatures, **and give the unification-`Var`
   an optional bounded constraint slot** (`number`/`comparable`) — this last
   part is decided *now*, in the same refactor, so we never repaint the `Var`
   representation. (ADR 0003 + ADR 0007, §4.)

2. **Introduce custom sum types** via a `type` declaration
   (`type Maybe a = Just a | Nothing`), which is how Elm defines `Maybe` and
   `Bool`. Constructors become values and patterns. `List` keeps literal syntax
   + built-in support (as in Elm). (ADR 0005, §6, §7.)

3. **Add `case … of`, `if … then … else`**, with **exhaustiveness checking** to
   preserve totality. (ADR 0005, §8.)

4. **Desugar sugar onto the core**: string interpolation → `String` concat;
   `if` → `case` on `Bool`; qualified built-ins (`List.map`, …) → entries in a
   prelude type-and-value environment. (ADR 0004, ADR 0006, §9, §10.)

5. **Numbers, Elm-faithful** (`Int`/`Float`, `number`/`comparable` constrained
   type variables, infix operators with Elm precedence, and the numeric/
   `comparable` builtins). This is the one bounded step outside pure HM, taken
   exactly as far as Elm takes it. (ADR 0007, §4.4, §9.5, §10.2.)

6. **Add two IR primitives** `file` and `lineInFile` — plain-`String` glyphs,
   with `mode` optional in the *surface* via `Maybe`, motivating the whole
   type-system push. (§11.)

The **crucial totality reframe** (ADR 0004/0005/0007): because there is **no
user recursion**, the standard Elm functions that a user would write recursively
(`List.map`, `List.foldr`, `Maybe.andThen`, `String.join`, …) — *and* the
numeric/`comparable` primitives (`round`, `abs`, `modBy`, `compare`, `min`, …) —
cannot be written in-language. They ship as **total built-in combinators** in a
prelude. This is a deliberate divergence-in-mechanism (not in naming/semantics):
the user writes exactly `List.map f xs` or `round x`, but it resolves to a
Rust-implemented total builtin, not to library source. Infix operators
(`+ * < == && …`) are the same story: they parse to applications of these
builtins. See §10 and §12.

---

## 4. Type representation (ADR 0003)

### 4.1 The new `Type` enum

Replace the current enum with:

```rust
pub enum Type {
    /// Unification variable (Algorithm W), now carrying an optional bound.
    /// `Constraint::None` is an ordinary free var; `Number`/`Comparable` are
    /// Elm's bounded type variables (§4.4). The bound is DECIDED IN WAVE 0 so
    /// the representation is never repainted, even though enforcement lands
    /// with numbers (ADR 0007).
    Var(u32, Constraint),
    /// A rigid/skolem type variable introduced by a signature's `a`, `b`, …
    /// Present only transiently while checking one signature; see §4.3.
    Rigid(String),
    /// Applied type constructor: name + arguments.
    ///   String         => Con("String", [])
    ///   Int, Float     => Con("Int", []), Con("Float", [])
    ///   Bool           => Con("Bool", [])
    ///   List a         => Con("List", [Var/…])
    ///   Maybe a        => Con("Maybe", [ … ])
    ///   AptPackage     => Con("AptPackage", [])
    ///   SystemdService => Con("SystemdService", [])
    ///   Glyph          => Con("Glyph", [])
    ///   File, LineInFile => Con("File", []), Con("LineInFile", [])
    Con(String, Vec<Type>),
    Fun(Box<Type>, Box<Type>),
    Record(BTreeMap<String, Type>),
}

/// A bound on a unification variable. `None` is an ordinary HM var.
/// `Number` admits `Int`/`Float`; `Comparable` admits
/// `Int`/`Float`/`String`/`Char`(if added)/`List comparable`/tuples-of-
/// comparable. This is the ONLY departure from pure HM, and it is bounded
/// exactly as Elm bounds it — no user-extensible constraints (§4.4, ADR 0007).
pub enum Constraint { None, Number, Comparable }
```

Notes and rationale:

- **The `Var` bound is a Wave-0 decision, enforced later.** Even though the first
  waves create only `Constraint::None` vars, the `Var(u32, Constraint)` shape is
  chosen *now* so numbers (ADR 0007) slot in additively: numeric-literal typing
  and the `comparable` builtins just start minting `Number`/`Comparable` vars,
  and `unify` already threads the bound. **This is the "pick it once, don't
  repaint" call the coordinator flagged.** A `Scheme`'s quantified vars must also
  remember their bound (so a generalized `number` re-instantiates as `number`);
  `Scheme.vars` becomes `Vec<(u32, Constraint)>` (or a parallel bound map).
- **`Con(String, Vec<Type>)` subsumes** `Str`(→`String`), `AptPackage`,
  `SystemdService`, `Glyph`, adds `Int`/`Float`, and *replaces* the bespoke
  `Glyphs` with `Con("List", [Con("Glyph", [])])`. One node covers nullary types
  (empty arg vec) and applied ones. Simpler than a separate `App`/`Con` split and
  matches how Elm's checker thinks (a type is a head constructor applied to
  arguments).
- **Naming — `String` not `Str` (DECIDED).** Elm's type is `String`; Emet
  adopts `Con("String", [])` and the `String` surface spelling. A transitional
  `Str` **alias** is accepted for one migration step so existing `.emet` files
  and tests do not all churn at once, then removed. This is settled, not an open
  question.
- **`Rigid(String)`** is new and exists to make **signature checking** correct
  for polymorphic signatures (§4.3). It never escapes into general inference.

### 4.2 Unify / apply / ftv / occurs / Display updates

All five functions in `infer.rs` currently pattern-match the concrete arms;
they collapse into `Con`/`Fun`/`Record`/`Var` cases:

- **`unify`**:
  - `(Con(n1, a1), Con(n2, a2))`: if `n1 == n2` and `a1.len() == a2.len()`,
    unify args pairwise; else type mismatch.
  - **Glyph subsumption arms become explicit special cases** on `Con` heads
    (this is the ADR-0002 permissive injection, preserved verbatim in behaviour):
    `(Con("AptPackage", []), Con("Glyph", []))` and its mirror unify; same for
    `SystemdService`; `AptPackage`/`SystemdService` still do **not** unify with
    each other. §5 explains how this interacts with `List Glyph`. **This is the
    interim shortcut** — sound only while nothing eliminates glyphs; see §5/§12
    and ADR 0008 for the deferred principled model.
  - **Bound-carrying `Var`s (`bind`):** when a `Var(v, c)` is bound to a type
    `t`, if `c` is `Number`/`Comparable` the binding is admissible only if `t`
    satisfies the bound (§4.4): a concrete `Con` head must be in the admissible
    set; another `Var(w, c2)` merges the bounds (`Number ∧ Comparable = Number`,
    since `Number ⊂ Comparable`; `None ∧ c = c`), keeping the bound on the
    survivor; an incompatible concrete (`String` for `Number`) is a type error.
    Occurs-check is unchanged. **Wave 0 threads the bound through `bind` even
    though only `None` bounds exist until numbers land** — so ADR 0007 adds only
    the admissibility table, not new plumbing.
  - `Rigid(a) ~ Rigid(a)` unify (same name only); `Rigid(a) ~ Rigid(b)` for
    `a≠b` is a mismatch. A `Rigid` unifying with a `Var` binds the var to the
    rigid (this is how a signature var flows into the body — but see §4.3 for
    the preferred skolemization approach that avoids even needing this arm).
- **`apply` / `ftv` / `occurs`**: recurse into `Con`'s arg vector exactly as
  they already recurse into `Fun`/`Record`. `Rigid` is a leaf (no ftv — it is
  *rigid*, i.e. not free to unify away; §4.3). A `Var`'s bound rides along with
  the var identity; `ftv`/`generalize`/`instantiate` preserve it.
- **`Display`**: `Con(n, [])` → `n`; `Con("List", [t])` → `List t` (and we may
  special-case `[t]` rendering if we keep list-bracket sugar); `Con(n, args)` →
  `n a b …` with parens around composite args; `Rigid(a)` → `a`; `Var(n, None)`
  → a lowercase `t{n}` as today (pretty-printed `a b c` in a `Scheme`); a bound
  `Var` renders as `number`/`comparable` (matching Elm's display of bounded
  vars).

### 4.3 Signatures with type variables — the one genuinely new inference bit

Today the signature is unified directly (`unify(inferred, sig)`), correct only
because sigs are ground. For `map : (a -> b) -> List a -> List b` we must:

1. **Parse** the signature into a `Type` where each lowercase type-var name maps
   to a `Rigid(name)` (consistently: every `a` in one signature is the same
   `Rigid("a")`).
2. **Check** the decl against it with proper *skolemization* so we neither over-
   nor under-constrain. The standard, minimal-machinery approach:
   - Infer the body to get `inferred` (a `Type` with `Var`s), as today.
   - **Instantiate the signature**: replace each distinct `Rigid(name)` with a
     *fresh unification `Var`*, giving `sig_inst`. Unify `inferred` with
     `sig_inst`.
   - **Then verify the signature is not more general than the body**: after
     unification, generalize the inferred type and check it is *at least as
     general* as the annotation (i.e. the annotation's quantified vars did not
     get forced to a concrete type). The pragmatic, low-machinery check used by
     small HM implementations: skolemize the signature vars to *fresh distinct
     rigid constants*, unify, and after unifying ensure **no two skolems were
     unified together and no skolem escaped** into the environment. If a skolem
     was forced to `String` (or to another skolem), the signature claimed more
     polymorphism than the body has → type error ("signature is too general").

   For Wave 1 the *simpler* half is sufficient and safe: instantiate signature
   `Rigid`s to fresh `Var`s and unify (accepts all correct programs; the only
   thing it fails to catch is a signature *more general* than the body, e.g.
   annotating a monomorphic body with `a -> a`). The full skolem-escape check is
   a hardening follow-up (§13, deferred). **Recommendation: ship the instantiate-
   and-unify version in Wave 1, add skolem-escape checking in a later wave**, and
   say so explicitly in the ADR — it is a known, bounded soundness gap
   (over-general signatures accepted), not an unsoundness in *evaluation*
   (a wrongly-general signature still evaluates fine; it just should have been
   rejected).

3. **Generalization already works** — once the body unifies with the
   instantiated signature, `generalize` quantifies the remaining free vars, and
   the decl becomes polymorphic exactly as `id` does today.

**This is the whole inference delta.** No changes to the core W algorithm; only
(a) the `Con` refactor, and (b) `Rigid` handling at the signature boundary.

### 4.4 Constrained type variables — `number` and `comparable` (ADR 0007)

Elm's numeric ergonomics rely on two **bounded** type variables. `3 : number`,
`3.0 : Float`, and `(+) : number -> number -> number` all work because `number`
ranges over `Int` and `Float`; `(<) : comparable -> comparable -> Bool` works
because `comparable` ranges over `Int`/`Float`/`String`/`Char`/`List
comparable`/tuples-of-comparable. This is the **one place Elm leaves pure HM** —
a bounded, typeclass-*like* constraint — and Emet follows it **exactly that
far**: two built-in constraints, no user-defined typeclasses, no `where`-clauses,
no dictionary passing.

Mechanism (all resting on the Wave-0 `Var(u32, Constraint)` decision):

- **Admissibility sets.**
  - `Number` admits `Int`, `Float`.
  - `Comparable` admits `Int`, `Float`, `String`, `Char` (if a `Char` type is
    added; not required for Wave-numbers), `List t` where `t` is comparable, and
    tuples of comparables (if tuples are added; not in confirmed scope).
  - `Number ⊂ Comparable`.
- **Numeric literals.** An integer literal (`3`) types as a **fresh `Var(_,
  Number)`** — so `3 : number`, usable at `Int` or `Float`. A float literal
  (`3.0`) types as `Con("Float", [])` directly (Elm: float literals are `Float`,
  not `number`). This is Elm-accurate.
- **`bind` enforces the bound** (as described in §4.2): binding a `Number` var to
  `String` fails; binding two constrained vars merges to the stronger bound.
- **Generalization/instantiation carry the bound.** A generalized `number` var
  re-instantiates as a fresh `number` var, so `(+)`'s polymorphism survives across
  use sites. This is why `Scheme` must store per-var bounds (§4.1).
- **Defaulting.** A top-level value whose type is still an unresolved `number`
  after inference defaults to `Int` (Elm's behaviour), so `x = 3` is `Int` unless
  used at `Float`. Defaulting runs once, at generalization of a top-level decl /
  at `main`. This is a small, well-scoped rule; document it in ADR 0007.

**Divergence flagged:** this is *not* pure HM. It is bounded exactly as Elm
bounds it (two constraints, closed set), and it is the only constraint machinery
Emet gains. The representation is fixed in Wave 0; ADR 0007 only adds the
admissibility table + literal typing + defaulting.

---

## 5. Glyph subsumption vs. `List a` (the interaction to get right)

Today `List` evaluation and the bespoke `Glyphs` type paper over a real
question: with a *generic* `List a`, what is the element type of
`[ aptPackage {…}, systemdService {…} ]`?

**Recommendation: keep the ADR-0002 injection, and make the list-literal rule
explicit at the `Con("List", [elem])` level.**

- A list literal `[e1, …, en]` introduces a fresh element var `elem` and unifies
  each `infer(ei)` with `elem`. For ordinary homogeneous lists this yields
  `List a` / `List String` / etc. exactly as Elm.
- **The glyph case is the one subsumption point.** When an element infers to
  `AptPackage` or `SystemdService`, unifying it with `elem` uses the permissive
  arm, so `elem` resolves to `Glyph` and the list is `List Glyph`. `Glyphs` is
  now literally `List Glyph` — the old bespoke type disappears, its behaviour
  preserved.
- **Why still sound today — and what the real hazard is (corrected framing).**
  Matching a glyph is not itself the danger: pattern-matching a sum value is a
  trivial `case`, and if glyphs were an ordinary sum it would be perfectly sound.
  The interim hazard is specifically the **symmetric permissive-injection
  shortcut** of ADR 0002: `unify` lets `AptPackage`/`SystemdService` *both* flow
  into `Glyph` **and** (because unification is symmetric) lets a `Glyph`-typed
  hole be satisfied by either concrete glyph, without tracking which. That
  shortcut is sound only while **nothing eliminates a glyph** — the moment a
  `case` inspects "which glyph is this?", the symmetric injection can hand it a
  value whose concrete identity was never pinned down. So the near-term rule is
  not "matching is forbidden because matching is unsafe"; it is "we haven't built
  glyph constructors/patterns yet, and the *injection shortcut* must be replaced
  by a principled model **before** any glyph elimination is added." Until then,
  no glyph patterns exist to write, so the shortcut stays sound. Constructors
  keep returning their **precise subtype** (`aptPackage … : AptPackage`,
  `… -> SystemdService`) — subtype precision is retained, not erased.
- **Keeping the door open, cheaply.** The Wave-0 foundation (`Con` heads +
  bound-carrying `Var`) is chosen so that a principled glyph model is **additive,
  not a repaint**: `Glyph` is already a named `Con` head with concrete `Con`
  heads relating to it, which is exactly the shape both principled routes below
  extend. Two routes, sketched (full design deferred to ADR 0008):
  - **(a) Polymorphic/row variants.** Model `Glyph` as an open variant/row
    (`[ AptPackage | SystemdService | … ]`); constructors inject with precise
    row types; `case` matches variant tags soundly with exhaustiveness over the
    row. Most faithful to "typed sum with subtyping"; heavier machinery (row
    unification).
  - **(b) Lightweight nominal subtyping with explicit subsumption.** Keep
    nominal `AptPackage <: Glyph`, but replace the *symmetric* unification arm
    with a **directed** subsumption check (concrete → `Glyph` only, never the
    reverse) plus a real elimination form that requires the scrutinee be the
    sum type and matches nominal tags. Less machinery; integrates with the
    current nominal `Con` heads almost directly.
  - **Recommendation when the time comes: (b)** — directed nominal subsumption is
    the smaller, more Emet-shaped step (glyphs are a closed, compiler-owned set,
    not user-extensible rows), and it removes exactly the one unsound thing (the
    symmetric arm) while keeping everything else. Revisit (a) only if
    user-defined open variants ever become a goal (not in scope).
- `main : List Glyph` (renderable as `Glyphs` if we keep the alias) still holds;
  `check_module`'s accept-set for `main` becomes "`List Glyph`, `Glyph`,
  `AptPackage`, `SystemdService`" expressed over `Con`.

**Divergence from Elm to flag:** Elm lists are strictly homogeneous with no
subtyping; Emet's `List Glyph` accepts concrete-glyph elements via injection.
This is a pre-existing, deliberate Emet-ism (ADR 0002), now expressed through
`Con`, and interim until the ADR-0008 principled model supersedes the symmetric
shortcut.

---

## 6. Built-in vs. user-defined types (`Maybe`, `Bool`, `List`)

**Question:** hardcode `Maybe`/`Bool`/`List`, or add a general custom-type
mechanism and define them there?

**Recommendation: add a minimal `type` declaration** (custom sum types) — it is
the Elm-faithful path and is *less* special-casing in the long run, because
`Maybe` and `Bool` become ordinary library declarations rather than compiler
built-ins. `List` stays partly built-in (literal syntax + builtin combinators),
exactly as in Elm (Elm's `List` has compiler-supported literals; `Maybe`/`Bool`
are ordinary `type`s in `elm/core`).

### 6.1 The `type` declaration

Surface (exact Elm):

```elm
type Maybe a = Just a | Nothing
type Bool = True | False
```

AST:

```rust
pub struct TypeDecl {
    pub name: String,           // "Maybe"
    pub params: Vec<String>,    // ["a"]
    pub variants: Vec<Variant>, // Just(a), Nothing
    pub span: Span,
}
pub struct Variant {
    pub name: String,           // "Just"
    pub fields: Vec<Type>,      // [Rigid("a")] ; Nothing => []
    pub span: Span,
}
```

A `Module` gains `type_decls: Vec<TypeDecl>` alongside `decls`. Type decls are
processed **before** value decls (they populate the constructor environment).

### 6.2 What each constructor contributes

For `type Maybe a = Just a | Nothing`, the checker derives:
- A type constructor `Maybe` of arity 1 (registered so `type_parser` accepts
  `Maybe t`).
- Two **value** constructors as prelude-like bindings:
  - `Just : ∀a. a -> Maybe a`
  - `Nothing : ∀a. Maybe a`
  These are `Scheme`s inserted into the top-level `TyEnv`, so `Just` and
  `Nothing` type exactly like ordinary polymorphic functions. Constructors are
  first-class values (so `List.map Just xs : List (Maybe a)` works — Elm
  behaviour).

### 6.3 Minimal-machinery recommendation

- **Wave with sum types (Wave 3):** implement `type` decls generally, and define
  `Maybe` and `Bool` **in an Emet prelude source string** compiled ahead of the
  user module (so they are real `type` decls, dog-fooding the mechanism), OR as
  programmatically-injected `TypeDecl`s if we prefer not to ship a prelude
  parser path yet. **Recommendation: inject them programmatically first**
  (fewer moving parts, no prelude-parsing bootstrap), then migrate to a prelude
  source file once `type` decls are proven. Either way they are *not* hardcoded
  in `unify`.
- **`List`** is not a `type` decl (it has literal syntax + is the element of the
  glyph story); it stays a built-in `Con("List", [_])` with literal support.

**Deferred (not "modeling Elm-lite"):** type aliases (`type alias`),
constructors with many type params beyond what `Maybe` needs, opaque
types/modules, comparables/`number` type classes. None are needed for the
confirmed feature set; list them as explicit non-goals in the ADR.
(Records-as-extensible-rows was deferred here too, but is no longer a
non-goal — it is implemented per ADR 0010.)

---

## 7. Constructors as values (representation)

**`Expr`** gains a constructor application node, and **`Value`** gains a
constructed value:

```rust
// ast.rs
pub enum Expr {
    // … existing …
    /// A data constructor by name, e.g. `Just`, `Nothing`, `True`, `False`.
    /// Applied via the existing Expr::App, so `Just x` is App(Ctor("Just"), x).
    Ctor(String),
    Case { scrutinee: Box<Spanned<Expr>>, arms: Vec<Arm> },
    If  { cond: Box<Spanned<Expr>>, then_: Box<Spanned<Expr>>, else_: Box<Spanned<Expr>> },
}
pub struct Arm { pub pat: Spanned<Pattern>, pub body: Spanned<Expr> }
pub enum Pattern {
    Wildcard,                                   // _
    Var(String),                                // binds
    Str(String),                                // literal "…"
    Ctor(String, Vec<Spanned<Pattern>>),        // Just x, Nothing, True
}
```

```rust
// eval.rs
pub enum Value {
    Str(String),
    Glyph(Glyph),
    List(Vec<Value>),        // generalizes Glyphs; see §7.1
    Record(BTreeMap<String, Value>),
    Closure { param, body, env },
    Data { ctor: String, args: Vec<Value> },   // Just v, Nothing, True/False
}
```

- **Constructors as values.** `Ctor("Just")` evaluates to a curried closure-like
  builder. Simplest total implementation: represent a saturated constructor as
  `Value::Data`, and treat an unsaturated constructor as a `Closure` chain that
  collects args then produces `Data`. Concretely, evaluation of `Ctor(name)`
  looks up the constructor's arity (from the type env captured at compile time,
  or a small ctor-arity table threaded into `eval`) and builds an
  arity-collecting closure; `Nothing`/`True`/`False` (arity 0) are immediately
  `Data { ctor, args: [] }`. Typing: `Ctor(name)` instantiates the constructor's
  `Scheme` (§6.2).

### 7.1 `Value::Glyphs` → `Value::List`

Replace the special-cased `Value::Glyphs(Vec<Glyph>)` with a general
`Value::List(Vec<Value>)`. The `main`-extraction in `run_module` walks the final
`List` and expects each element to be a `Value::Glyph` (guaranteed by typing:
`main : List Glyph`), flattening to `Vec<Glyph>`. List *literals* no longer
flatten `Glyphs`-into-`Glyph` at eval time; instead the **glyph subsumption is a
type-level fact** and at runtime a `List Glyph` is just a `List` of `Glyph`
values. (This is cleaner than today's eval-time flattening and removes a special
case.)

**Totality note:** `Value::Data` is finite and non-recursive to build (no user
recursion, constructors are strict, evaluation terminates). No change to the
termination argument.

---

## 8. `case … of`, `if`, and exhaustiveness (ADR 0005)

### 8.1 Grammar and layout

```elm
case scrut of
    Just x  -> f x
    Nothing -> default
```

- Lexer: add keywords `case`, `if`, `then`, `else`, `type`. `of` already exists
  and already opens a layout block in `layout.rs::opens_layout`.
- **Layout (the ADR-0001 caveat comes due).** `case … of` opens an implicit
  block after `of` (already handled by `opens_layout`), whose arms are separated
  by the normal same-column `VSemi` rule and closed by dedent. **Unlike `let`,
  `case` has no `in`**, so the block closes purely by dedent — this needs **no
  new `parse-error(t)` rule**, which is the easy case. The one wrinkle: a
  single-line `case x of A -> a` inside a larger expression. Recommend requiring
  `case` arms to be laid out (each arm on its own line, or the whole thing in
  explicit `{ … ; … }`) for Wave-with-case, and defer inline single-line `case`
  until we decide whether a `parse-error(t)`-style close is worth it. Document
  this in ADR 0005 as an explicit, small syntactic restriction vs. Elm (Elm
  allows one-line `case` via its own layout). `if/then/else` need **no** layout
  (they are ordinary keywords in an expression), so `if` is layout-free.
- Parser: `case_expr` = `case` expr `of` block-of-arms; `arm` = pattern `->`
  expr; `if_expr` = `if` expr `then` expr `else` expr. Patterns parse as: `_`,
  lowercase ident (`Var`), `Upper` ident + sub-patterns (`Ctor`), string literal,
  and the **list patterns** `[]` (empty list), `(x :: xs)` (head/tail, mirroring
  the `::` expression operator), and `[a, b, …]` — the last desugaring to nested
  `x :: (y :: [])`, so only the two constructors `[]`/`::` reach inference and
  matching.

### 8.2 Inference

- `if c then t else e`: unify `infer(c)` with `Bool`; unify `infer(t)` with
  `infer(e)`; result is that type. (`if` is literally sugar for `case c of True
  -> t ; False -> e` — implement it as desugaring to keep one code path, §9.)
- `case scrut of arms`:
  - `st = infer(scrut)`.
  - For each arm, **infer the pattern against `st`**, which *binds* pattern
    variables into the arm's env: a `Ctor(name, subpats)` pattern instantiates
    the constructor's scheme, unifies its result type with `st`, and unifies each
    field type with the corresponding sub-pattern; a `Var(x)` binds `x : st`; `_`
    binds nothing; a `Str` literal unifies `st` with `String`.
  - All arm bodies unify to one result type.

### 8.3 Exhaustiveness + redundancy (totality-critical)

**This is the load-bearing totality mechanism** and must be in the same wave as
`case`. A non-exhaustive `case` would let evaluation reach a scrutinee with no
matching arm — a runtime "no match" failure, which **breaks totality**. So:

- **Exhaustiveness is a compile error, not a runtime fallthrough.** After typing
  a `case`, run an exhaustiveness check over the head constructors of the
  scrutinee's type:
  - If `st` is a known sum type `T` with constructor set `C`, the arms must
    cover every constructor in `C` (a `Var`/`_`/var-pattern arm covers the
    remainder). Missing constructors → error listing them (Elm-style message).
    `List` participates as a synthetic two-constructor sum `{ [], :: }`, so a
    `case` on a list is exhaustive exactly when it covers both `[]` and
    `(x :: xs)` — no list-specific branch in the checker.
  - If `st` is `String` (infinite domain), exhaustiveness **requires** a
    catch-all (`_` or a var pattern), since string literals can never be
    exhaustive. Missing catch-all → error.
  - Nested patterns: Wave-with-case can use a **simplified** check — treat only
    the top-level constructor for coverage and require sub-patterns to be
    var/`_` OR recurse one level. Full Maranget-style usefulness matrices are
    the "correct" algorithm but are more than we need for `Maybe`/`Bool` and
    shallow matches. **Recommendation: implement the standard, well-understood
    *usefulness* algorithm (Maranget 2007) restricted to our small pattern
    language** — it is a few dozen lines, gives both exhaustiveness *and*
    redundancy for free, and future-proofs nested matches. If time-boxed, ship
    the shallow check first and note the limitation.
- **Redundancy (unreachable arms):** an arm that matches nothing new (e.g. a
  second `Nothing ->`, or any arm after a catch-all) is a **warning or error**.
  Elm makes redundant patterns an error; recommend **error** to stay strict and
  total-minded. The usefulness algorithm yields this directly.
- **No `_`-fallthrough-to-crash, ever.** There is no runtime "pattern match
  failed" path; the type-checker guarantees a match exists. `eval` for `case`
  can `unreachable!()` on no-match (mirroring the existing `unreachable!` style
  for the other impossible-by-typing cases in `eval.rs`).

**Totality statement for the ADR:** with exhaustive `case` + no recursion +
strict finite constructors + total built-in combinators, evaluation still always
terminates and produces a finite `Vec<Glyph>`. `case` adds *branching*, not
*looping*.

---

## 9. String interpolation (ADR 0004, the intentional divergence)

Surface: `"port ${port}"`, `"unit ${name}.service"`, where the thing inside
`${ … }` is the **full expression grammar** and must be **`String`-typed**.

### 9.1 Lexing

The cleanest low-machinery approach that fits the existing hand-lexer: **lex an
interpolated string into a small token sequence**, not a single `Str`. When the
lexer is inside a `"…"` and hits an unescaped `${`, it emits:

- `Tok::StrPart(String)` for the literal chunk before `${` (may be empty),
- a `Tok::InterpStart` (`${`),
- then lexes normal tokens for the embedded expression until the matching `}`,
- `Tok::InterpEnd` (`}`),
- continues the string, emitting further `StrPart`s / interps,
- a final `Tok::StrPart` (or reuse `Tok::Str` for a fully-literal string with no
  interpolation, preserving today's tokenization for the common case).

Matching the `}` requires the lexer to **count brace depth** inside `${ … }` so
that record/`case` braces in the embedded expression don't prematurely close the
interpolation. This is the only real lexer complexity; it is local and bounded.

Alternative considered: lex the whole `"…"` as one `Str` token carrying an
un-parsed body, then re-lex/re-parse interpolations in a second pass. Rejected:
it duplicates lexer logic and complicates spans. Emitting sub-tokens keeps one
lexer and gives correct spans into the embedded expression for `ariadne`.

**Escape for a literal `${`.** Recommend **`\${`** → literal `${` (consistent
with the existing backslash-escape convention in the lexer, where `\x` already
maps to `x` for unknown escapes; we make `$` special *only* when followed by
`{`, so a lone `$` stays literal and needs no escaping — matching common shell/JS
intuition). Justify in the ADR: `\${` reads naturally, requires no new escape
metacharacter, and dovetails with the current escape table. (Secondary option
`$${` → `${` was considered but adds a second escaping scheme; rejected for one
consistent backslash convention.)

### 9.2 Parsing + desugaring

Parse an interpolated literal into `StrPart`/embedded-`Expr` segments, then
**desugar to concatenation** before it leaves the parser (or in a tiny desugar
pass): `"a${e}b"` becomes `String.concat [ "a", e, "b" ]` (equivalently a fold
of `String.append` / `++`). A no-interpolation `"abc"` stays `Expr::Str("abc")`
(zero overhead, unchanged path).

- The desugar target is the **prelude `String.concat`** (§10), so interpolation
  has no dedicated `Expr` node — it lowers to ordinary `App`/`List`/`Str`. This
  keeps `infer.rs` and `eval.rs` **completely unchanged** for interpolation:
  they only ever see concatenation.

### 9.3 Typing

Because interpolation desugars to `String.concat [ … ]` and `String.concat :
List String -> String`, **each embedded expression is unified with `String`
automatically** by ordinary inference. A non-`String` interpolant (`"${n}"`
where `n : Int` or `n : Maybe String`) is a normal type error at the concat
site — and since numbers are now in scope, the common `"${port}"` with `port :
Int` is exactly such an error, resolved by the user with `String.fromInt port`
(un-deferred in §10.2). No special typing rule needed — the desugaring *is* the
typing rule. (This is the elegant payoff of desugaring rather than adding an
`Interp` node.)

### 9.4 IR consequence — no templating (the principle to enshrine)

Because interpolation is fully evaluated to a concrete `String` **before** the
value reaches a glyph, the IR **never** sees a placeholder. `file { contents =
"port ${port}" }` produces `Glyph::File { contents: "port 8080" }` with a
concrete string. **The IR carries only fully-evaluated concrete `String`s** — no
`${}`, no template DSL, explicitly **not** Ansible/Jinja. This extends the
existing "no JSON/YAML intermediary — evaluate straight to the IR" philosophy to
"**no templating layer — the language IS the generator; the IR is inert
concrete data.**" This principle is the heart of ADR 0004 and constrains all
future primitives: every glyph field is a concrete `String` produced by the
total language, never a template.

### 9.5 Numeric literals and infix operators (ADR 0007)

This subsection lives next to interpolation only because both touch the lexer;
the substance is ADR 0007.

**Lexing (new literal + operator paths).** The lexer today has *no* numeric
literals and *no* operator symbols beyond punctuation.

- **Integer literal** `[0-9]+` → `Tok::IntLit(i64)`. **Float literal**
  `[0-9]+ '.' [0-9]+` (and optionally `e` exponents; Elm requires a digit on both
  sides of the `.`) → `Tok::FloatLit(f64)`. Care: `.` is already `Tok::Dot`
  (field access) — a `.` is a float point only when flanked by digits with no
  intervening space; otherwise it stays `Dot`. A leading `-` is **not** part of a
  literal (Elm treats unary minus specially; `negate`/parenthesized `-x` cover
  it) — keep `-` an operator to avoid `x-1` ambiguity.
- **Operator symbols.** Add a maximal-munch operator lexer over the symbol set
  `+ - * / // ^ < > <= >= == /= && || ++`. `->` stays the arrow; `--` stays a
  line comment (lex comment before operator). Emit `Tok::Op(String)`; the parser
  maps each to its prelude builtin name.

**Parsing — a precedence layer (Pratt / precedence-climbing).** Insert one
precedence-climbing layer **between application and lambda/let/case** in
`expr_parser`: application binds tightest (as today), then binary operators by
Elm precedence, then the low-precedence forms. chumsky supports this directly via
`pratt`/`foldl` chains; no new crate. Elm-accurate precedence/associativity:

| Prec | Operators | Assoc | Meaning |
|---|---|---|---|
| 7 | `^` | right | power |
| 7 | `*` `/` `//` | left | mul, float-div, int-div |
| 6 | `+` `-` | left | add, sub |
| 5 | `++` | right | append (List/String) |
| 5 | `::` | right | cons — prepend an element onto a list (→ `cons`) |
| 4 | `==` `/=` `<` `>` `<=` `>=` | non-assoc | equality, comparison |
| 3 | `&&` | right | boolean and |
| 2 | `||` | right | boolean or |

(`not` is a prefix *function*, not an operator: `not : Bool -> Bool`.)
Non-associative level 4 means `a < b < c` is a parse error, exactly as Elm.
Every operator **desugars to application of a prelude builtin** (`a + b` →
`add a b` internally, `a ++ b` → `append a b`, `a == b` → `eq a b`), so
`infer.rs`/`eval.rs` see only ordinary `App`; the operators' *types* are the
builtins' types (`(+) : number -> number -> number`, `(<) : comparable ->
comparable -> Bool`, `(==) : comparable -> comparable -> Bool` — note Elm's `==`
is `comparable`-ish/`equatable`; Emet takes it as `comparable` for the closed
set, no user equality). `(++)` is overloaded in Elm across `List`/`String`; Emet
gives it `appendable`-like behaviour via **two prelude entries** or a third tiny
constraint — **recommendation: keep it simple and give `++` the two concrete
builtins `String.append`/`List.append` selected by the operand type at a single
`Appendable` constraint**, mirroring Elm's `appendable` but closed. Flag in ADR
0007 whether to add `Appendable` as a third constraint or special-case `++` in
the operator desugarer; recommend the latter (special-case `++` resolution) to
avoid a third constraint unless it proves necessary.

**Totality:** operators are strict total builtins; no new looping. Division by
zero must be **total** — follow Elm: integer `//` and `modBy 0` in Elm return `0`
(or are defined) rather than trapping; adopt Elm's exact totality behaviour for
`//`, `modBy`, `remainderBy` so evaluation cannot crash. State the chosen
semantics in ADR 0007.

---

## 10. Module-qualified built-ins (ADR 0006)

`List.map`, `Maybe.withDefault`, `String.concat` are **not** real modules or
records. They are **compile-time-resolved qualified names**: the parser reads
`Upper "." lowerIdent` as a single qualified identifier `"List.map"`, and the
prelude binds that exact string in both the type env (a `Scheme`) and the value
env (a Rust built-in).

### 10.1 Parsing `Upper.lowerIdent`

- Lexer already produces `Tok::Upper("List")`, `Tok::Dot`, `Tok::Ident("map")`.
- In `expr_parser`, add an atom: `Tok::Upper` **immediately followed by**
  `Tok::Dot` **and** `Tok::Ident` → `Expr::Var("List.map")` (a qualified name).
  The "immediately followed by" (no whitespace/adjacency via spans) disambiguates
  from record field access `.f` on an uppercase-typed value (of which there are
  none today, but keep it clean). Because the whole thing becomes a plain
  `Expr::Var` with a dotted name, **no new `Expr` node, no eval change** — it
  resolves through the ordinary environment lookup.
- Disambiguating `map`: there is no bare `map` in the prelude (only `List.map`,
  `Maybe.map`), so qualification is mandatory and there is no ambiguity. This is
  intentional and matches how a beginner uses Elm with explicit qualification.

### 10.2 Built-in functions and exact signatures (Wave-graded)

All signatures are Elm-accurate. `String` is Emet's `Con("String",[])`;
`number`/`comparable` are bounded vars (§4.4).

**`List` (Elm `List.*`):**
```
List.map        : (a -> b) -> List a -> List b
List.filter     : (a -> Bool) -> List a -> List a
List.foldr      : (a -> b -> b) -> b -> List a -> b
List.foldl      : (a -> b -> b) -> b -> List a -> b
List.concat     : List (List a) -> List a
List.concatMap  : (a -> List b) -> List a -> List b
List.append     : List a -> List a -> List a
List.length     : List a -> Int
List.isEmpty    : List a -> Bool
List.member     : comparable -> List comparable -> Bool
List.range      : Int -> Int -> List Int
List.sum        : List number -> number
List.maximum    : List comparable -> Maybe comparable
List.minimum    : List comparable -> Maybe comparable
```

**`Maybe` (Elm `Maybe.*`):**
```
Maybe.map       : (a -> b) -> Maybe a -> Maybe b
Maybe.withDefault : a -> Maybe a -> a
Maybe.andThen   : (a -> Maybe b) -> Maybe a -> Maybe b
```

**`String` (Elm `String.*`):**
```
String.concat   : List String -> String
String.join     : String -> List String -> String
String.append   : String -> String -> String
String.isEmpty  : String -> Bool
String.length   : String -> Int
String.toUpper  : String -> String
String.toLower  : String -> String
String.split    : String -> String -> List String   -- (Elm: sep -> str)
String.fromInt  : Int -> String
String.fromFloat: Float -> String
String.toInt    : String -> Maybe Int
String.toFloat  : String -> Maybe Float
```

**Numeric + `comparable` (Elm's global scope — bare, NOT qualified):**
Elm exposes these unqualified (`round`, `abs`, `min`, …); Emet binds them as
bare prelude names (the one exception to "qualification is mandatory", because
Elm itself exposes them bare). They desugar targets for operators live here too.
```
toFloat     : Int -> Float
round       : Float -> Int
floor       : Float -> Int
ceiling     : Float -> Int
truncate    : Float -> Int
negate      : number -> number
abs         : number -> number
modBy       : Int -> Int -> Int
remainderBy : Int -> Int -> Int
clamp       : number -> number -> number -> number
min         : comparable -> comparable -> comparable
max         : comparable -> comparable -> comparable
compare     : comparable -> comparable -> Order      -- Order = LT | EQ | GT
not         : Bool -> Bool
-- operator desugar targets (not user-visible names; bound internally):
add sub mul  : number -> number -> number
fdiv         : Float -> Float -> Float     -- (/)
idiv         : Int -> Int -> Int           -- (//), total: idiv _ 0 = 0 (Elm)
pow          : number -> number -> number  -- (^)
lt gt le ge  : comparable -> comparable -> Bool
eq neq       : comparable -> comparable -> Bool        -- (==) (/=)
and or       : Bool -> Bool -> Bool
```
`compare` returns `Order = LT | EQ | GT`, an ordinary sum type (defined via the
`type` mechanism, §6), so it needs no special machinery.

Notes:
- **Numeric functions are now un-deferred** (numbers are in scope). `List.length`,
  `List.range`, `String.length`, and equality-dependent `List.member` all ship
  with the numbers wave; they were the reason numbers had been a prerequisite.
- `List.member`/`==` use the closed `comparable` set — **no user-defined
  equality**; comparing functions/records is a type error (as in Elm).
- Each builtin is a total Rust function over `Value`; higher-order ones apply a
  `Value::Closure` via the existing apply path in `eval.rs`. Because there is no
  user recursion, these builtins are the *only* way to iterate a list or fold a
  number — which is precisely why they must be built-ins (§12).

### 10.3 Prelude wiring

Introduce a `prelude` module that returns `(TyEnv, Env)` seeded with: the two
glyph constructors (existing behaviour, moved here or kept special), the sum-type
constructors (`Just`/`Nothing`/`True`/`False`), and every qualified builtin
above. `check_module` starts from the prelude `TyEnv` instead of
`TyEnv::default()`; `run_module` starts from the prelude `Env`. Builtins are
represented in `Env` as a new `Value::Builtin { name, arity, apply: fn }` or as
pre-built curried closures over Rust fns — recommend a `Value::Builtin` variant
holding a function pointer and collected args (keeps `eval`'s `App` arm simple:
apply to a builtin collects until saturated, then calls the Rust fn).

---

## 11. `file` and `lineInFile` primitives (§ motivating examples)

**IR (`src/ir.rs`), all fields plain `String`:**
```rust
pub enum Glyph {
    AptPackage { name: String },
    SystemdService { unit: String },
    File { path: String, contents: String, mode: String },   // key file:<path>
    LineInFile { path: String, line: String },               // key fileline:<path>:<line>
}
```
- `key()`: `format!("file:{path}")`, `format!("fileline:{path}:{line}")`.
- `describe()`: analogous human strings.
- `analyze` dedup keeps working: two identical `File`s at one path dedup; two
  *different* `File`s at the same path conflict (desired).

**Surface constructors** (reserved lowercase words, exactly like
`aptPackage`/`systemdService`, special-cased in `parse_atom`):
```elm
file       { path = String, contents = String, mode = String }  : File
lineInFile { path = String, line = String }                     : LineInFile
```
- New `Con("File", [])` / `Con("LineInFile", [])` types; both inject into
  `Glyph` via the same (interim, symmetric) permissive arm (extend the injection
  set in `unify` and the `main` accept-set). New `Expr::File { path, contents,
  mode }` / `Expr::LineInFile { path, line }` (or a generalized `Expr::Prim(name,
  fields)` to stop growing the enum per primitive — see recommendation below).
  Note: these add glyph *types*, not glyph *patterns* — no elimination form, so
  the ADR-0002 injection stays sound (§5, ADR 0008).

**`mode` as an optional field via `Maybe` — the whole reason the type work comes
first.** In the *surface* language, `mode` is naturally optional. Two ways:
1. Keep the `file` primitive's `mode` a required `String` at the IR boundary, and
   let a **userland helper** provide the ergonomics:
   ```elm
   fileWithMode : String -> String -> Maybe String -> File
   fileWithMode path contents mode =
       file { path = path, contents = contents, mode = Maybe.withDefault "0644" mode }
   ```
   This keeps the primitive's IR fields all-required-`String` (matching the "IR
   is inert concrete data" principle) while the *surface* gets `Maybe`-typed
   optionality. **Recommended**: the primitive is dumb; ergonomics live in
   Emet, demonstrating exactly why generics + `Maybe` precede a nice `file`.
2. Make the `file` constructor itself accept an optional `mode` field. More
   special-casing in the parser; less clean. Not recommended for Wave-file.

**Generalizing primitive constructors (recommendation):** rather than one
`Expr`/parser branch per primitive (4 and counting), introduce a small table
`PRIMITIVES: &[(name, required_fields, build: fn(fields) -> Expr/Glyph)]` so
adding a primitive is a data change, not new enum arms + new parser branches.
This is optional polish; flag it, do it when adding `file` so the third/fourth
primitive doesn't multiply the special-casing.

---

## 12. Every divergence from Elm, and why

| Feature | Elm | Emet | Why |
|---|---|---|---|
| **String interpolation** | none | `"${expr}"` desugars to `String.concat` | Confirmed intentional ergonomics; the one additive divergence. |
| **`List.map` / numeric fns as source** | library functions, user-writable via recursion | **built-in total combinators** | **No user recursion** (totality) at design time; ADR 0011 later allowed self-recursion, but these still ship as builtins. Naming/semantics identical; only the *implementation locus* differs. |
| **`List Glyph` element subtyping** | strictly homogeneous, no subtyping | concrete `AptPackage`/`SystemdService` inject into `Glyph` via a **symmetric** unify arm | ADR 0002 glyph sum. The interim *symmetric injection* is sound only while glyphs have no elimination form; a principled model (ADR 0008) supersedes it before any glyph `case`. Matching itself is not the hazard — the symmetric shortcut is. |
| **Output** | a program/HTML/values | a `Vec<Glyph>` OS-resource IR | Emet's entire purpose; the IR is the sole, inert output. |
| **No templating** | n/a | IR carries only concrete evaluated `String`s | ADR 0004 principle: the language is the generator; explicitly not Jinja/Ansible. |
| **`number`/`comparable`** | bounded type vars (typeclass-*like*) | **same, closed to two constraints** | Follow Elm exactly this far; **no user-defined typeclasses**, no `where`/dictionaries (ADR 0007). |
| **Infix operators** | full operator set + user-defined operators | Elm's built-in operator set with Elm precedence; **no user-defined operators** | Operators desugar to prelude builtins; user-defined operators are a non-goal. |
| **Recursion / `Task`/`Cmd`/effects / ports / `type alias` / records-as-rows / user typeclasses / user operators** | present | **self-recursion now allowed (ADR 0011); rest absent** | Totality was the original reason recursion was a non-goal; ADR 0011 prioritized ergonomics over totality and allows self-recursion. Mutual recursion and the other items remain non-goals. |
| **`case` layout** | one-line `case` allowed | Wave-with-case may require laid-out arms | Avoids a second `parse-error(t)` rule initially (ADR 0001 caveat). Small, documented restriction. |
| **`String` spelling** | `String` | `String` (with `Str` alias for one migration step) | Match Elm; `Str` alias eases migration. **Decided.** |
| **Numbers** | `Int`/`Float`/`number`/`comparable` | **same — in scope, Elm-faithful** | Now in scope; `List.length`/`range`/`String.length`/`String.fromInt` etc. ship with the numbers wave. |
| **Division totality** | `//`/`modBy 0` defined (return `0`) | **same** | Totality: evaluation must not trap; adopt Elm's exact defined behaviour (ADR 0007). |

---

## 13. Staged implementation plan

Each wave is independently compilable and leaves `cargo test` green. Ordering
follows the natural dependency chain: generics → List → Maybe/Bool → case/if →
**numbers/operators** → interpolation → file/lineInFile. Numbers slot in *after*
`case`/`Bool` (comparisons return `Bool`; `compare` returns an `Order` sum type)
and *before* interpolation (so `String.fromInt` exists for `"${port}"`
ergonomics), but their **type-representation cost is paid entirely in Wave 0** —
the constrained `Var` is decided there so numbers add only the admissibility
table, literal typing, operators, and builtins, never a `Type` repaint.

### Wave 0 — `Type` representation refactor (pure internal, no surface change)
- Replace `Type` variants with `Con(String, Vec<Type>)` + `Rigid(String)` **and
  make the unification var `Var(u32, Constraint)`** with `Constraint::{None,
  Number, Comparable}` (§4.1). Only `None` is minted until the numbers wave, but
  the representation, `bind`'s bound-threading, and `Scheme`'s per-var bounds are
  all put in place now. **This is the "decide the constrained-Var rep once"
  requirement.**
- Update `unify`/`apply`/`ftv`/`occurs`/`Display`/`generalize`/`instantiate` in
  `infer.rs` for `Con` + the bound-carrying `Var`; update `type_parser` to emit
  `Con` for the existing fixed names and `[Glyph]`.
- Replace `Value::Glyphs` with `Value::List` (or keep `Glyphs` variant for now
  and defer to Wave 2 — pick to minimize churn).
- **Green criterion:** all existing tests pass byte-for-byte in behaviour
  (`Str`/`String`, `Glyph`, `Glyphs`, `[Glyph]`, records still infer/eval
  identically). `Str` accepted as an alias for `String`.
- **No user-visible change** beyond the `Str`/`String` alias. This de-risks
  everything after it — *especially* numbers, whose only Wave-0 footprint is the
  `Constraint` enum and the bound plumbing (inert until Wave 5-numbers).

### Wave 1 — **Generics in signatures (minimal genuinely-usable)** ⟵ recommended MVP
- `type_parser`: accept lowercase type-var idents (→ `Rigid`) and applied
  constructors `Name t1 t2` and `List t` / `Maybe t` heads (parse the head +
  args; arity checked against known type constructors).
- `infer_decls`: instantiate a signature's `Rigid`s to fresh `Var`s, then unify
  (the "accept correct programs" half of §4.3). Generalization already works.
- **Deliverable value:** a user can now *write* `id : a -> a`, `const : a -> b ->
  a`, and polymorphic helper signatures — the inference already supported them;
  now the syntax does too. `List a` in signatures parses (even before builtins).
- **Green criterion:** new tests: `id : a -> a` checks; `const : a -> b -> a`
  checks; a wrong monomorphic use is still a type error.
- **This is the smallest wave that delivers a real new capability** and unblocks
  everything else. Recommended stopping point for a first shippable increment.

### Wave 2 — `List a` first-class + `List.` builtins + prelude
- Generalize list literals to `Con("List", [elem])` with the glyph-injection
  rule (§5); `Value::List`.
- Add the prelude scaffolding (`(TyEnv, Env)` seed; `Value::Builtin`).
- Ship `List.map/filter/foldr/foldl/concat/concatMap/append/isEmpty`.
- **Green:** `List.map webserver names`, `List.filter`, `List.concatMap` produce
  correct glyph lists; `main : List Glyph` holds.

### Wave 3 — custom sum types + `Maybe` + `Bool` + `Maybe.`/constructors
- `type` decls; constructors as values (`Ctor`, `Value::Data`); inject `Maybe`
  and `Bool` (programmatically first, then optionally a prelude source).
- `Maybe.map/withDefault/andThen`.
- **Green:** `Maybe.withDefault "0644" (Just "0700")`, `Just`/`Nothing`
  round-trip; `Bool` values exist.
- (No `case` yet — values only; still useful via the `Maybe.` builtins.)

### Wave 4 — `case … of`, `if … then … else`, exhaustiveness
- Lexer keywords `case/if/then/else/type` (some added in Wave 3); layout for the
  `of` block (dedent-close, no new `parse-error(t)`); parser for arms/patterns.
- Inference for patterns + arms; `if` desugars to `case … of True/False`.
- **Exhaustiveness + redundancy checking** (Maranget-restricted, §8.3) — same
  wave, non-negotiable for totality.
- **Green:** exhaustive `case` on `Maybe`/`Bool` checks; non-exhaustive is a
  compile error; redundant arm is an error.

### Wave 5 — **numbers, `number`/`comparable`, infix operators** (ADR 0007)
Depends on Wave 0 (the `Constraint` slot already exists), Wave 2 (prelude +
`Value::Builtin` + `List`), and Wave 4 (`Bool` for comparisons; `Order`/`Maybe`
sum types for `compare`/`List.maximum`). Two loosely-coupled halves that can be
built in parallel once the above land:
- **5a — numeric types + constraint enforcement.** Lexer `IntLit`/`FloatLit`
  (§9.5); `Con("Int",[])`/`Con("Float",[])`; literal typing (`3 : number`,
  `3.0 : Float`); `bind` enforces `Number`/`Comparable` admissibility; top-level
  `number` **defaulting to `Int`**. Numeric + `comparable` builtins (§10.2):
  `toFloat/round/floor/ceiling/truncate/negate/abs/modBy/remainderBy/clamp/min/
  max/compare`, and the **un-deferred** `List.length/range/sum/maximum/minimum`,
  `List.member`, `String.length/fromInt/fromFloat/toInt/toFloat`. Division
  totality per Elm (§9.5).
- **5b — infix operators.** Operator lexing (maximal munch, §9.5); the
  precedence-climbing layer in `expr_parser` with Elm precedence/associativity
  (§9.5 table); each operator desugars to a prelude builtin from 5a. `++`
  resolution special-cased (§9.5).
- **Green:** `x = 3 + 4 * 2` is `Int` `11`; `y = 3.0 / 2.0` is `Float`;
  `"a" ++ "b"`; `2 < 3` is `Bool`; `List.sum [1,2,3]`; `String.fromInt 8080`;
  `"x" + 1` and `a < b < c` are type/parse errors; `x = 5` defaults to `Int`.

### Wave 6 — string interpolation
- Lexer sub-tokenization of `"…${…}…"` with brace-depth matching and `\${`
  escape; parser assembles segments; desugar to `String.concat` (needs Wave 2's
  `String.concat` + `List`). With numbers landed (Wave 5), `"${port}"` for
  `port : Int` is a type error resolved via `String.fromInt port` — the intended
  Elm-faithful ergonomic.
- Also ship any remaining `String.` builtins.
- **Green:** `"port ${p}"` with `p : String` → concrete string in a `File`/glyph;
  non-`String` interpolant is a type error; `\${` is literal.

### Wave 7 — `file` + `lineInFile` + `Scroll` (per-host container)
- **`file`/`lineInFile` glyphs.** IR variants + `key`/`describe`/`analyze`;
  surface constructors (ideally via the `PRIMITIVES` table, §11); inject into
  `Glyph`; `mode` optional via userland `Maybe.withDefault` helper (needs
  Waves 2–3).
- **`Scroll` per-host container (ADR 0009).** Add the `Scroll` output container
  and shift `main`'s bottom to `List Scroll` (see §13.1 below). Depends **only**
  on Wave 2 (`List` first-class), so it could land independently — but it is
  placed here, alongside `file`/`lineInFile`, deliberately: files are glyphs that
  live *inside* a scroll (the two are neighbours), and grouping the shift lets the
  demo + `examples/*.emet` migrate to `[ scroll { name = …, glyphs = [ … ] } ]`
  exactly **once** rather than churning them twice.
- **Green:** `file`/`lineInFile` produce correct glyphs; `fileWithMode` helper
  compiles; conflicting `File`s at one path flagged by `analyze` **within a
  scroll**; `main : List Scroll` checks; two scrolls sharing a glyph key do **not**
  conflict.

#### 13.1 — `Scroll`: the per-host output container (ADR 0009)

The program's output bottom changes from a flat `List Glyph` to **`List Scroll`**,
where a `Scroll` is the per-host grouping of glyphs.

- **IR node.** `Scroll { name: String, glyphs: Vec<Glyph> }` in `src/ir.rs` — an
  **opaque, nominal** node one level *above* `Glyph` (a scroll *contains* glyphs).
  Start with just `name` + `glyphs`; richer machine attributes (IPv4/IPv6/hostname)
  are deferred and add without disturbing this shape.
- **Surface constructor.** A reserved lowercase record constructor
  `scroll { name = String, glyphs = List Glyph } : Scroll`, in the same family as
  `aptPackage`/`systemdService` — a nominal opaque node, **not** a general record.
- **Type.** `Scroll` is `Con("Scroll", [])`; it is **not** a glyph and does **not**
  inject into `Glyph` (contrast the glyph subsumption of §5 / ADR 0002).
- **`main : List Scroll`.** The sole output shape. A single-host config is
  `[ scroll {…} ]`. `run_module` produces `Vec<Scroll>`; `lib.rs`'s `Compiled` and
  `main.rs` rendering group output per scroll.
- **Per-scroll `analyze`.** Glyph key-conflict detection moves *inside* each
  scroll, so two hosts may share glyph keys (both install `nginx`) without a false
  conflict. `name` is a label only for now; no cross-scroll uniqueness is enforced
  yet (that arrives with the machine-attributes work).

### Later / deferred (explicit non-goals for "Elm-lite")
- Skolem-escape checking for over-general signatures (§4.3 hardening).
- One-line inline `case` (needs a `parse-error(t)`-style close rule per ADR
  0001).
- **Glyph pattern-matching** — planned but deferred; foundation kept
  variant-ready. Requires replacing ADR 0002's symmetric injection with the
  principled model of §5 / ADR 0008 (recommended: directed nominal subsumption)
  **before** any glyph elimination is added.
- User-defined operators, user-defined typeclasses / additional constraints
  beyond `number`/`comparable`, `type alias`, opaque types, effects/`Task`/
  `Cmd`/ports. (Extensible records/rows was listed here too, but is no longer
  a non-goal — it is implemented per ADR 0010.)
- Prelude-as-source-file bootstrap (migrate from programmatic injection).
- `Char` type and tuple types (would extend the `comparable` set; not required).

### Parallelization
- Wave 0 must land first (everything depends on `Con` + the `Constraint` slot).
- Waves 1 and 2 can overlap partly (signatures vs. list runtime) but 2's prelude
  benefits from 1's signature parser for writing builtin schemes cleanly.
- Wave 4 (`case`) depends on Wave 3 (sum types to match on).
- Wave 5 (numbers) depends on Waves 0/2/4; its two halves 5a/5b parallelize.
  **5b (infix parsing) is largely independent of the type system** and could be
  prototyped against 5a's builtin signatures before 5a's enforcement is finished.
- Wave 6 (interpolation) depends on Wave 2 (`String.concat`); reads better after
  Wave 5 so `String.fromInt` exists, but does not hard-depend on it.
- Wave 7 depends on Waves 2–3 for the `file`/`lineInFile` `Maybe`-mode ergonomics;
  the IR variants alone could land independently. Its `Scroll` half (ADR 0009)
  depends **only** on Wave 2 (`List` first-class) and could land earlier or in
  parallel; it is grouped here so the `main : List Scroll` shift and the
  example migration happen once, next to the `file` glyphs that live inside a
  scroll.

---

## 14. Biggest risks and open questions (for the user)

**Decided this revision (no longer open):**
- **`Str → String`: decided** — rename to `String` with a transitional `Str`
  alias for one migration step, then remove the alias.
- **Numbers: decided** — fully in scope, Elm-faithful (`Int`/`Float`,
  `number`/`comparable`, infix operators). No minimal-subset compromise.

**Still open / for awareness:**

1. **`case` layout restriction.** Requiring laid-out arms (no one-line `case`)
   in Wave 4 avoids a second `parse-error(t)` rule (ADR 0001's known debt). Is a
   one-line `case` important enough to pay for the extra layout rule now?
   Recommendation: defer; ship laid-out `case` first. **Needs user call.**
2. **Exhaustiveness algorithm depth.** Full Maranget usefulness (nested
   patterns + redundancy) vs. a shallow top-level-constructor check. The former
   is the right long-term answer and not much more code; the latter is faster to
   ship but limits nested matches. Recommendation: Maranget-restricted; fall
   back to shallow only if time-boxed. **Design opinion; user may weigh in.**
3. **Signature generality gap (§4.3).** Wave 1 accepts *over-general* signatures
   (`f : a -> a` on a monomorphic body) until skolem-escape checking lands.
   This is a soundness-of-*rejection* gap, not an evaluation bug. Acceptable as
   staged? Recommendation: yes, with the ADR noting it. **User awareness.**
4. **Constructor/builtin value representation.** `Value::Data` +
   `Value::Builtin` vs. encoding everything as `Closure`. Recommendation:
   explicit `Data`/`Builtin` variants (clearer eval, better totality argument).
   Low-risk, but it does grow the `Value` enum. **Design opinion.**
5. **New — `number` defaulting scope.** Elm defaults an unresolved `number` to
   `Int`. Emet must pick *where* defaulting runs (per top-level decl at
   generalization, and at `main`). A too-eager rule could reject programs Elm
   accepts; a too-lazy one leaves ambiguous types. Recommendation: default at
   top-level-decl generalization + at `main`, matching Elm; validate against a
   handful of mixed `Int`/`Float` examples. **Design opinion; low risk.**
6. **New — `++` overloading (`appendable`).** Elm's `++` is `appendable`
   (`String`/`List`). Emet can special-case `++` resolution in the operator
   desugarer (recommended) or add a third `Appendable` constraint. Special-casing
   avoids a third constraint but is slightly ad-hoc. **Design opinion.**
7. **New — division/`modBy 0` totality semantics.** To stay total, `//`,
   `modBy`, `remainderBy` must be defined at `0` (Elm returns `0`). Confirm we
   adopt Elm's exact behaviour rather than, say, erroring. Recommendation: match
   Elm exactly. **User awareness.**
8. **Glyph pattern-matching kept OPEN, deferred (corrected framing).** The
   design does **not** forbid glyph matching forever; it defers it. The only
   interim unsoundness is ADR 0002's **symmetric injection under elimination**,
   not matching per se. Near-term: keep concrete-subtype + injection (sound while
   nothing eliminates glyphs); constructors keep precise subtypes. When glyph
   `case` is wanted, replace the symmetric arm with the principled model of §5 /
   ADR 0008 (**recommended: directed nominal subsumption**). The Wave-0
   foundation is chosen so this is additive, not a repaint. **User awareness; no
   action needed now.**

---

## 15. Summary

The inference engine is already polymorphic; this program is largely surface
syntax and desugaring over a stable HM core, plus two genuinely new pieces —
exhaustiveness checking for `case` (protects totality) and Elm's bounded
`number`/`comparable` vars (the one bounded step outside pure HM). The keystone
is the Wave-0 refactor: `Type::Con` **plus the constrained `Var(u32,
Constraint)`**, decided once so numbers slot in additively and are never a
repaint. **Wave 1 (generic signatures) remains the recommended minimal
genuinely-usable increment**; numbers/operators land as Wave 5 (after `case`/
`Bool`, before interpolation), and each wave adds one Elm-faithful capability
while preserving totality, the inert-IR principle, and — interim — the glyph
subsumption whose symmetric shortcut a principled model (ADR 0008) will
supersede before any glyph elimination.
