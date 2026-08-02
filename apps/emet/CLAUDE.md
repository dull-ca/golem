# CLAUDE.md

Guidance for working in Emet. The compiler crate lives at `apps/emet/` and the
LSP at `apps/emet-lsp/` — hover, completion, and go-to-definition, all served
from the compiler's own inference engine (ADR 0018), so the editor and `emetc`
never disagree. Both are members of the **golem** monorepo's Cargo workspace.
This file is Emet-only — golem has its own root `CLAUDE.md` for the wider
project. File paths below are relative to `apps/emet/`; source paths are under
`apps/emet/src/`.

## Where Emet fits

Emet is the authoring language: a program compiles to a fleet of scrolls, the
desired state for a set of hosts. `golemd` (elsewhere in this monorepo) is what
enacts scrolls on real hosts; the compiler's binary, content-addressed output
(ADR 0012) is the wire contract between them. Emet supersedes Nickel
(`nickel/lib.ncl` in golem) as the authoring surface — see `docs/TODO.md` for
the integration backlog.

## What this is

`emet` — a **typed, functional configuration language** modeled on
Elm, with totality as a design preference rather than a guarantee (self-recursion
is allowed; ADR 0011). It compiles to the glyph IR — the `Glyph`/`Scroll` model,
which lives in the shared `scroll-format` crate and is re-exported through
`apps/emet/src/ir.rs`. `emetc`'s default output is the binary, content-addressed
manifest of those scrolls (ADR 0012/0013); the readable plan is `--text`, JSON is
`--json`. The surface is Elm-lite:
top-level decls with optional signatures, Hindley-Milner inference with generics,
records with row-polymorphic access and update, `let`, lambdas, `case`/`if`,
patterns in argument position, numbers with infix operators, string
interpolation, and the offside (layout) rule. A program evaluates to a **fleet of
scrolls** — `main : List Scroll`, one `Scroll` per host, each a recursive tree of
glyphs or named sub-scrolls (ADR 0031). There is no JSON/YAML intermediary and no
templating layer.

## Dev environment

The toolchain comes from **devenv** (`devenv.nix` at the golem repo root) and
auto-loads via **direnv** (`.envrc`). rustc is not on the base system PATH.

- Interactive shell: `cd` into the golem repo (direnv loads the shell), then
  `cargo …`.
- One-off / scripted: prefix with `direnv exec . …`, e.g.
  `direnv exec . cargo test -p emet -p emet-lsp`.
- No direnv: `devenv shell` then `cargo …`.

## Build / test / run

Commands are workspace-scoped (run from the golem repo root, or anywhere in the
workspace); `-p emet` selects the crate:

```
cargo build -p emet
cargo test -p emet                                   # emet crate tests
cargo test -p emet -p emet-lsp                       # + the LSP crate
cargo run -p emet                                    # built-in demo
cargo run -p emet -- apps/emet/examples/basic.emet   # run a file (.emet extension)
```

## Pipeline (each stage is one module)

```
lexer.rs   chars -> tokens (line/col; string parts + ${…} interpolation;
           numeric literals; operator symbols; case/if/then/else/of keywords)
header.rs  peels the column-zero `module … exposing` + `import` lines off the
           token stream BEFORE layout (they do not follow the offside rule);
           yields the header + the body tokens still to be laid out
layout.rs  offside rule (Haskell 2010 §10.3): virtual { } ; + parse-error(t)
parser.rs  chumsky over the laid-out tokens -> Module (exprs, patterns, types,
           signatures, records + record update, operators by precedence);
           `param_parser` is the deliberately narrow parameter grammar (ADR 0044)
infer.rs   Algorithm W: unify / generalize / instantiate; signature + generics
           checks; number/comparable constraints; case exhaustiveness/redundancy;
           `reject_refutable_param` for parameter patterns
eval.rs    typed Module -> Vec<Scroll> (may not terminate; depth-guarded)
resolve.rs multi-module stage (ADR 0016): load the import graph from disk over
           the `emet.json` library search path (ADR 0024; `manifest.rs`), reject
           cycles, order imports before importers, then check + eval each module
           against the harvested interfaces of what it imports
prelude.rs (TyEnv, Env) for constructors (Just/Nothing/True/False/LT/EQ/GT) and
           the total built-in combinators (List./Maybe./String./Char./Tuple./
           numeric/compare); the Elm-faithful Char/String surface is
           scalar-indexed to agree with String.length's chars().count()
           (ADR 0025); the Tuple module + String.uncons ride the tuple type
           (ADR 0027)
lib.rs     compile() drives the single-file stages; compile_file() runs the
           multi-module resolve stage; analyze() does per-leaf-unit IR checks
           (a glyph-key conflict is scoped to one leaf; siblings may share a key)
main.rs    CLI (`emetc build`); default emits the binary manifest (stdout/`-o`),
           `--text` the readable plan, `--json` the debug view, or all
           diagnostics (ariadne) — one report per error (ADR 0022)
```

## Module system (ADR 0016)

An Elm-shaped, minimal module system for reuse across files:

- **`module Name exposing (..)`** / `exposing (a, B, Type(..))` header, one
  module per file, **file path = module name**. The header is optional: a file
  with no `module` line is a valid entry module that exposes everything.
- **`import Foo` resolves over a search path (ADR 0024, `manifest.rs`):** the
  entry file's own directory first, then each `source-directories` entry of the
  nearest `emet.json` (found by walking up from the entry file), first
  `Foo.emet` winning. No `emet.json` = entry-directory-only, the original ADR
  0016 behavior. The repo-root `emet.json` names `lib/`, so any entry resolves
  `import Quadlet` to `lib/Quadlet.emet`.
- **`import Foo` / `import Foo as F` / `import Foo exposing (bar)`.** Qualified
  access `Foo.bar` reuses the same dotted-name resolution as built-ins
  (`List.map`; ADR 0006). Only exposed values, types, and — for `Type(..)` —
  constructors are importable. An open-exposed (`Type(..)`) constructor is fully
  usable in an importer: both to build values and to **pattern-match**, with the
  exhaustiveness checker seeing the imported type's complete constructor set. A
  type exposed without `(..)` stays unmatchable — its constructors never enter
  the importer (`resolve::import_constructors`, `infer::seed_imported_constructors`).
- **Exactly one module has `main`** (the entry); the rest are libraries. A
  library that declares `main` is a compile error.
- **`compile(src)`** is the single-file pipeline (imports not resolved from
  disk); **`compile_file(entry)`** runs the resolve stage over the import graph.

## Language / type system

- **HM + generics.** Signatures may mention type variables and applied
  constructors (`map : (a -> b) -> List a -> List b`); let-generalization makes
  unannotated decls polymorphic. Signatures are checked against the inferred type
  (instantiate-and-unify), and a second pass rejects a signature more general
  than its body — the skolem-escape check (ADR 0021).
- **Types.** `String`, `Char`, `Int`, `Float`, `Bool`,
  `Order`, `List a`, `Maybe a`, records, tuples (`(A, B)` / `(A, B, C)`, 2–3
  elements; unit `()` is the empty tuple; 4+ is a parse error → use a record),
  functions; the glyph types `AptPackage`,
  `SystemdService`, `Filesystem`, `LineInFile` and their sum `Glyph`; the
  filesystem `Entry` sum (`File`/`Directory`/`Symlink`); and the recursive-scroll
  types `Scroll`, `Contents` (a scroll's `glyphs`-xor-`groups`), `Policy`, and
  `OnExhaust` (ADR 0031), all first-class so a library can compute a group tree
  or a policy and pass it in.
  (`Filesystem` is the single first-class type for `file`/`directory`/`symlink`;
  the entry kind is the `Entry` sum, carried on a `Filesystem` glyph's `entry`
  field; ADR 0019.)
  (The transitional `Str`/`Glyphs` aliases are gone — `String` and `List Glyph`
  are the sole spellings; `Str`/`Glyphs` are now unknown-type errors.)
  `Char` is one Unicode scalar (char literals `'c'`, escapes
  `'\n'`/`'\t'`/`'\\'`/`'\''`/`'\u{...}'`), comparable/orderable by codepoint
  like Elm; it is authoring-time only and never reaches the wire (ADR 0025).
- **`number` / `comparable` / `appendable`.** Elm's three bounded type
  variables — the one place emet leaves pure HM. No user-defined typeclasses.
  Integer literals are `number` (default `Int`); float literals are `Float`.
  `appendable` (`String`/`List a`) backs `++`; it shares no admissible type with
  `number` or `comparable`, so `merge_constraints` rejects `appendable ∧ number`
  and `appendable ∧ comparable`. Tuples are **structurally** `comparable` — a
  tuple satisfies `comparable` iff every element does, compared lexicographically;
  unit is vacuously comparable (ADR 0027). This is the one non-flat
  `constraint_admits` case: it recurses into the elements rather than matching a
  `Con` head.
- **`case … of` + `if`.** Compile-time **exhaustiveness and redundancy** checking;
  a non-exhaustive or redundant match is a compile error. `if` desugars to `case`
  on `Bool`. Arms must be laid out (inline single-line `case` is deferred).
  Patterns (`ast.rs::Pattern`): `_` (wildcard), a lowercase binder, string,
  integer, and char literals (typed `number`/`Char`; negative ints like `-1`
  allowed — ADR 0026), `Upper p …` (constructor), the list patterns `[]`
  (`Nil`), `(head :: tail)` (`Cons`), and `[a, b, …]` (nested `Cons` ending in
  `Nil`), and tuple patterns `(a, b)` / `(a, b, c)` and unit `()` (ADR 0027 — a
  single-shape product, so a tuple `case` needs no catch-all when its element
  patterns are exhaustive). **`Float` literal patterns are a compile error** (Elm-faithful:
  IEEE-754 equality is unreliable, so branch on floats with `<`/`>` in an `if`).
  `List` is treated as a two-constructor sum (`[]` / `::`) so the exhaustiveness
  checker requires both cases — see below.
- **Patterns in argument position (ADR 0044).** A function, `let`, or lambda
  parameter may be a constructor pattern — `unwrap (Box held) = held`,
  `\(Box spec) -> spec.label` — for a **single-constructor** type only. A plain
  name is the `Pattern::Var` case of the same path. Multi-constructor types stay
  `case`-only: a parameter has no sibling arms, so admitting a refutable one
  would reintroduce the partial functions ADR 0005 forbids.
  `infer::reject_refutable_param` is the gate, an exhaustive match with no
  catch-all that recurses into sub-patterns; `parser::param_parser` is narrower
  than `pattern_parser` so the refutable spellings cannot be written at all.
  Nullary constructors need their parens (`f (Unit) = …`) — the one divergence
  from Elm here.
- **Record update (ADR 0044).** `{ r | field = value, … }` copies a record with
  the named fields replaced, over ADR 0010's rows: the base unifies with an
  **open** record demanding those fields, so an open base stays open and one
  setter serves every record shape carrying them. **Type-preserving** — a new
  value must have the field's existing type, and the result type is the base's,
  so an update never reshapes a record (write a literal for that). Updating an
  absent field is a type error naming it. The base may be any expression, not
  only a variable — a deliberate superset of Elm.
- **Numbers + operators.** `+ - * / // ^ < > <= >= == /= && || ++ ::` with Elm
  precedence; operators desugar to prelude built-ins. `::` (cons) is
  right-associative at level 5, desugaring to the `cons` builtin. `++` (append)
  is also right-associative at level 5, desugaring to the `append` builtin
  (`∀p:appendable. p -> p -> p`), which dispatches String vs. List on the
  runtime value at eval time — no surface spelling of its own, reached only via
  `++`. `/` is float division, `//` integer; division / `modBy` / `remainderBy`
  by zero return `0` (total).
- **String interpolation.** `"port ${expr}"` (embedded expr must be `String`);
  desugars to `String.concat`. The IR carries only fully-evaluated concrete
  strings — no templating (not Ansible/Jinja).

`Maybe`, `Bool`, and `Order` are built-in sum types injected via the prelude
constructor registry. User-facing `type Foo a = …` declarations parse and infer:
each constructor scheme is generalized over the type's params, the type
constructor registers at its arity, and both nullary and parameterized (arity>0)
user types cross module boundaries (`register_type_decls`, ADR 0016).

## Invariants — do not drift

- **Total language (soft preference, not a guarantee).** Self- *and* mutual
  recursion are allowed (ADR 0011): decls are grouped into dependency SCCs
  (`depgraph`) and inferred/evaluated per group, so neither self- nor mutual
  recursion is guaranteed to terminate. Totality is a design preference, not an
  invariant. Exhaustive `case` is still enforced at compile time regardless.
- **The IR is inert, concrete data.** `apps/emet/src/ir.rs` re-exports the
  `Glyph`/`Scroll` model from the shared `scroll-format` crate (the wire
  contract, ADR 0013); those types' field/variant order is a versioned contract,
  not a free refactor. Adding capability = new IR variants + reconcilers; the
  *language* is unchanged.
- **No JSON/YAML, no templating.** Every glyph field is a concrete `String`
  produced by the language.
- **Small dependency footprint.** `ariadne` for diagnostics, `chumsky` for the
  parser — that's it.

## The two subtle subsystems

Treat `apps/emet/tests/layout.rs` and `apps/emet/tests/pipeline.rs` as the spec — fix the
implementation, not the tests, unless a test is provably wrong.

- **Layout (`layout.rs`).** Dedent / virtual-`;` / the `parse-error(t)` handshake
  with the parser. `Layout::close_implicit` lets the parser pop an implicit
  context and splice a virtual `}` when it is stuck — this is what makes
  single-line `let x = e in e` parse. `of` opens a `case` block (closed by
  dedent, no `in`); adding new layout-opening keywords needs new close rules.
- **Algorithm W (`infer.rs`).** generalize / instantiate, signature unification
  with rigid vars for generics plus the skolem-escape check that rejects an
  over-general signature (`check_signature_generality`, ADR 0021), the
  `number`/`comparable`/`appendable` constraint
  bounds threaded through `bind`, the per-SCC group inference (`depgraph` +
  `infer_group`) that supports self/mutual recursion by binding a group
  monomorphic before generalizing it against the pre-group env, and the `case`
  exhaustiveness/redundancy check (what
  removes the runtime "no match" path; kept independent of totality per ADR 0011).
  The check is Maranget's usefulness algorithm over the sum constructors; `List`
  joins it as a synthetic two-constructor sum (`[]` / `::`, via
  `prelude::sum_type_constructors` and `constructor_scheme`), so list patterns
  are checked like any other sum with no list-specific code in the checker.

## The four primitives and `Scroll`

Reserved lowercase record constructors (lexed as `Tok::Ident`, special-cased
in `parser.rs::parse_atom`), all IR fields plain `String` except a filesystem
entry's `mode`, which lowers to a `u16` `Perms` in `eval` (ADR 0019):

```
aptPackage     { name }                    -> Glyph::AptPackage      key apt:<name>
systemdService { unit }                    -> Glyph::SystemdService  key systemd:<unit>
file           { path, contents, mode }    -> Glyph::Filesystem      key file:<path>
directory      { path, mode }              -> Glyph::Filesystem      key file:<path>
symlink        { path, target }            -> Glyph::Filesystem      key file:<path>
lineInFile     { path, line }              -> Glyph::LineInFile      key fileline:<path>:<line>
```

`scroll`, `rollback`, `keep`, and `retry` join the reserved lowercase set
(`is_reserved_constructor`). `rollback` / `keep` build a `Policy` braceless (the
build/match split of ADR 0017); `retry { maxAttempts, baseDelayMs,
backoffMultiplier, maxDelayMs, jitterFraction, maxElapsedMs, onExhaust }` sets
the retry knobs — camelCase surface fields that lower to the snake_case wire
`Policy` (ADR 0031 §3).

Glyphs and the filesystem `Entry` are **matchable** (ADR 0017): a `case` may
destructure a built glyph or entry. The spelling splits by direction — the
reserved lowercase words *build* (`aptPackage`, `systemdService`, `file`,
`directory`, `symlink`, `lineInFile`), the PascalCase tags *match* (`AptPackage`,
`SystemdService`, `Filesystem`, `LineInFile`; and on the entry, `File`,
`Directory`, `Symlink`). The match tags exist only as patterns — they are not
bound as values, so they cannot construct. Matching stays sound because a
concrete glyph widens one-way into `Glyph` (never back), so a `Glyph`-typed
value is always known to hold the sum, not an un-pinned variant.

`file`/`directory`/`symlink` are three spellings of the one filesystem glyph,
each building a different `Entry` arm (`build_constructor` enforces the per-arm
field set — `symlink` takes no `mode`, `directory` no `contents`); together with
`aptPackage`/`systemdService`/`lineInFile` they are still **four glyph kinds**.
Each injects into `Glyph`. A `Scroll` is a recursive, strict tree (ADR 0031):
`scroll { name, glyphs = [ … ] }` is a **leaf** unit of glyphs, and `scroll
{ name, groups = [ … ] }` a **branch** of named sub-scrolls — exactly one of
`glyphs` or `groups`, never both, plus an optional `policy`. `main : List
Scroll` is the fleet, one root scroll per host. `analyze` (`lib.rs`) is **per
leaf unit**: conflicting declarations for one glyph key *within a single leaf*
are an error; sibling leaves (and separate scrolls) may share a key. In the
golem legend the creature is animated by inscribed glyphs — each primitive is
one glyph, a scroll is one host's marks, the fleet is the complete desired
state.

## Apply order

golemd enacts in **source order**: a host's leaf units in the order they appear
in your Emet source (ADR 0031 §2), and within a unit its glyphs in source order —
installs and replaces first, removes last (in reverse source order, so teardown
unwinds the opposite way to setup). Ordering is **author-controlled**: if unit B
must come after unit A, or glyph B after glyph A, write B after A. There is **no
dependency DAG** and no automatic reordering, across units or within one — your
source order is the whole contract (ADR 0029 §6, ADR 0031).

## Pointers

- `docs/design/0001-elm-lite-type-system-and-value-language.md` — the design.
- `docs/adr/0001`–`0009` — the decisions (chumsky/layout split; the glyph
  primitives; generics; no-templating + interpolation; `case`/exhaustiveness;
  module-qualified built-ins; numbers/constrained vars/operators; deferred glyph
  matching; `Scroll`).
- `docs/TODO.md` — intentional deferrals and known gaps.

## Working style

Small commits per fix; run `cargo test -p emet -p emet-lsp` after each.
</content>
