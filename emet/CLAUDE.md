# CLAUDE.md

Guidance for working in Emet. Emet is a subtree of the **golem** monorepo; the
crate lives at `crates/emet/` and the LSP at `crates/emet-lsp/`, both members of
golem's Cargo workspace. This file is Emet-only — golem has its own root
`CLAUDE.md` for the wider project. File paths below are relative to the Emet
subtree root (`emet/`); source paths are under `crates/emet/src/`.

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
is allowed; ADR 0011). Its sole output is the glyph IR (`crates/emet/src/ir.rs`). The surface is Elm-lite:
top-level decls with optional signatures, Hindley-Milner inference with generics,
records, `let`, lambdas, `case`/`if`, numbers with infix operators, string
interpolation, and the offside (layout) rule. A program evaluates to a **fleet of
scrolls** — `main : List Scroll`, one `Scroll` per host, each a list of glyphs.
There is no JSON/YAML intermediary and no templating layer.

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
cargo run -p emet -- crates/emet/examples/basic.emet # run a file (.emet extension)
```

## Pipeline (each stage is one module)

```
lexer.rs   chars -> tokens (line/col; string parts + ${…} interpolation;
           numeric literals; operator symbols; case/if/then/else/of keywords)
layout.rs  offside rule (Haskell 2010 §10.3): virtual { } ; + parse-error(t)
parser.rs  chumsky over the laid-out tokens -> Module (exprs, patterns, types,
           signatures, records, operators by precedence)
infer.rs   Algorithm W: unify / generalize / instantiate; signature + generics
           checks; number/comparable constraints; case exhaustiveness/redundancy
eval.rs    typed Module -> Vec<Scroll> (may not terminate; depth-guarded)
prelude.rs (TyEnv, Env) for constructors (Just/Nothing/True/False/LT/EQ/GT) and
           the total built-in combinators (List./Maybe./String./numeric/compare)
lib.rs     compile() drives all stages; analyze() does per-scroll IR checks
main.rs    CLI; renders the plan or the first error (ariadne), per scroll
```

## Language / type system

- **HM + generics.** Signatures may mention type variables and applied
  constructors (`map : (a -> b) -> List a -> List b`); let-generalization makes
  unannotated decls polymorphic. Signatures are checked against the inferred type
  (instantiate-and-unify; the skolem-escape check for over-general signatures is
  deferred — see `docs/TODO.md`).
- **Types.** `String` (`Str` a transitional alias), `Int`, `Float`, `Bool`,
  `Order`, `List a`, `Maybe a`, records, functions; the glyph types `AptPackage`,
  `SystemdService`, `File`, `LineInFile` and their sum `Glyph`; and `Scroll`.
- **`number` / `comparable`.** Elm's two bounded type variables — the one place
  emet leaves pure HM. No user-defined typeclasses. Integer literals are
  `number` (default `Int`); float literals are `Float`.
- **`case … of` + `if`.** Compile-time **exhaustiveness and redundancy** checking;
  a non-exhaustive or redundant match is a compile error. `if` desugars to `case`
  on `Bool`. Arms must be laid out (inline single-line `case` is deferred).
- **Numbers + operators.** `+ - * / // ^ < > <= >= == /= && || ++` with Elm
  precedence; operators desugar to prelude built-ins. `/` is float division,
  `//` integer; division / `modBy` / `remainderBy` by zero return `0` (total).
- **String interpolation.** `"port ${expr}"` (embedded expr must be `String`);
  desugars to `String.concat`. The IR carries only fully-evaluated concrete
  strings — no templating (not Ansible/Jinja).

`Maybe`, `Bool`, and `Order` are built-in sum types injected via the prelude
constructor registry; user-facing `type Foo a = …` declarations are designed but
not yet parsed (`docs/TODO.md`).

## Invariants — do not drift

- **Total language (soft preference, not a guarantee).** Self-recursion is now
  allowed (ADR 0011), so evaluation is no longer guaranteed to terminate;
  totality is a design preference, not an invariant. Exhaustive `case` is still
  enforced at compile time regardless. Mutual recursion remains unsupported
  (decls inferred left-to-right).
- **`crates/emet/src/ir.rs` is the sole, inert output.** Adding capability = new IR variants
  + reconcilers; the *language* is unchanged.
- **No JSON/YAML, no templating.** Every glyph field is a concrete `String`
  produced by the language.
- **Small dependency footprint.** `ariadne` for diagnostics, `chumsky` for the
  parser — that's it.

## The two subtle subsystems

Treat `crates/emet/tests/layout.rs` and `crates/emet/tests/pipeline.rs` as the spec — fix the
implementation, not the tests, unless a test is provably wrong.

- **Layout (`layout.rs`).** Dedent / virtual-`;` / the `parse-error(t)` handshake
  with the parser. `Layout::close_implicit` lets the parser pop an implicit
  context and splice a virtual `}` when it is stuck — this is what makes
  single-line `let x = e in e` parse. `of` opens a `case` block (closed by
  dedent, no `in`); adding new layout-opening keywords needs new close rules.
- **Algorithm W (`infer.rs`).** generalize / instantiate, signature unification
  with rigid vars for generics, the `number`/`comparable` constraint bounds
  threaded through `bind`, and the `case` exhaustiveness/redundancy check (what
  removes the runtime "no match" path; kept independent of totality per ADR 0011).

## The four primitives and `Scroll`

Four reserved lowercase record constructors (lexed as `Tok::Ident`, special-cased
in `parser.rs::parse_atom`), all IR fields plain `String`:

```
aptPackage     { name }                    -> Glyph::AptPackage      key apt:<name>
systemdService { unit }                    -> Glyph::SystemdService  key systemd:<unit>
file           { path, contents, mode }    -> Glyph::File            key file:<path>
lineInFile     { path, line }              -> Glyph::LineInFile      key fileline:<path>:<line>
```

Each injects into `Glyph`. `scroll { name, glyphs }` groups one host's glyphs
into a `Scroll`, and `main : List Scroll` is the fleet. `analyze` (`lib.rs`) is
**per scroll**: conflicting declarations for one glyph key within a scroll are an
error; two scrolls may share a key. In the golem legend the creature is animated
by inscribed glyphs — each primitive is one glyph, a scroll is one host's marks,
the fleet is the complete desired state.

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
