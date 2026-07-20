# 0004-no-templating-ir-and-string-interpolation

## Status

Proposed

## Context

Emet's design philosophy (CLAUDE.md, ADR 0002) is "no JSON/YAML intermediary —
evaluate straight to the IR." As primitives grow to carry file contents and
config text (`file { contents = … }`, `lineInFile`), there is an obvious
temptation to let the IR carry *templates* — placeholders like `${port}` or a
Jinja/Ansible-style mini-language resolved later. That would reintroduce exactly
the untyped, partially-evaluated intermediary the project rejects.

Separately, the language wants **string interpolation** — `"port ${port}"` —
which Elm does not have. This is the one intentional divergence from Elm.

These two concerns are the same decision viewed from both ends: *where does
string construction happen, and what reaches the IR?*

## Decision

**Two halves of one principle: interpolation is a fully-evaluated language
feature, and the IR carries only concrete strings.**

### The no-templating IR principle

The IR (`src/ir.rs`, the `Glyph` enum) carries **only fully-evaluated concrete
`String`s** — no placeholders, no template DSL, explicitly **not** Ansible/Jinja.
All computation (interpolation, conditionals, optional fields, string building)
happens in the typed, total language and is **fully evaluated before a value
reaches the IR**. This extends "no JSON/YAML intermediary" to its conclusion:
**no templating layer — the language *is* the generator; the IR is inert
concrete data.** Every glyph field is a concrete `String` produced by evaluation,
never a template. This constrains all future primitives.

### String interpolation (the intentional Elm divergence)

- Surface: `"port ${expr}"`, where `expr` is the **full expression grammar** and
  must itself be **`String`-typed**.
- **Lexing:** an interpolated string is lexed into a small token sequence —
  literal chunks (`StrPart`) interleaved with `${` / embedded tokens / `}` — with
  the lexer counting brace depth inside `${ … }` so record/`case` braces do not
  close the interpolation prematurely. A string with no `${` lexes to a single
  `Str` exactly as today (unchanged common path).
- **Escape for a literal `${`: `\${`** → literal `${`. This reuses the existing
  backslash-escape convention (the lexer already maps `\x` → `x` for unknown
  escapes); `$` is special *only* before `{`, so a lone `$` stays literal. One
  consistent backslash convention, no new metacharacter.
- **Desugaring:** `"a${e}b"` lowers to `String.concat [ "a", e, "b" ]` in the
  parser/a tiny desugar pass. There is **no `Interp` AST node**; interpolation
  becomes ordinary `App`/`List`/`Str`, so `infer.rs` and `eval.rs` need **no
  interpolation-specific code**.
- **Typing falls out for free:** `String.concat : List String -> String`, so
  each embedded expression is unified with `String` by ordinary inference; a
  non-`String` interpolant is a normal type error at the concat site.

Because interpolation is fully evaluated to a concrete `String` before it can
reach a glyph field, the IR never sees a placeholder — closing the loop with the
no-templating principle above.

## Alternatives considered

1. **Template-carrying IR** (`contents` holds `${port}`, resolved by a later
   reconciler). Rejected: reintroduces an untyped, partially-evaluated
   intermediary; defeats "evaluate straight to the IR"; is precisely the
   Jinja/Ansible model the project rejects.
2. **A dedicated `Expr::Interp` node** with its own inference/eval rules.
   Rejected: more machinery for no benefit; desugaring to `String.concat` reuses
   existing typing and evaluation entirely.
3. **`$${` as the literal-`${` escape** (doubling). Rejected: introduces a
   second escaping scheme alongside the existing backslash escapes; `\${` keeps
   one convention.
4. **Lex the whole `"…"` as one opaque `Str` and re-parse interpolations in a
   second pass.** Rejected: duplicates lexer logic and muddies spans; emitting
   sub-tokens keeps a single lexer and gives correct spans into embedded
   expressions for `ariadne`.

## Consequences

- The IR stays inert and fully concrete; reconcilers never interpret templates.
  This is now a stated invariant future primitives must honour.
- Interpolation adds real but *local* lexer complexity (brace-depth matching);
  everything downstream is unchanged because it desugars to concatenation.
- Interpolation depends on `String.concat` existing in the prelude (see the
  design doc's Wave 2 / Wave 5 ordering).
- The `\${` escape is the one new lexer escape; documented and tested.
- Cross-references ADR 0002 (evaluate-straight-to-IR) and the design doc
  `docs/design/0001-…` §9.
