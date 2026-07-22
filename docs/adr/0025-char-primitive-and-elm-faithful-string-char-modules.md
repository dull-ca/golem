# 0025-char-primitive-and-elm-faithful-string-char-modules

## Status

Accepted — implemented, tests green.

As built, two negligible divergences from the plan above: `String.words` splits
on Rust's `char::is_whitespace` (Unicode) rather than JS `\s`, differing only on
exotic separators; and `String.lines` also breaks on a lone `\r`, though Emet has
no `\r` escape to author one directly.

## Context

Emet's `String` surface today is a handful of builtins in `prelude.rs` —
`String.append`/`concat`/`join`/`length`/`fromInt`/`fromFloat`/`toInt`/`toFloat`
plus the polymorphic `++`/`append` (`appendable`). There is no way to take a
string apart: no `split`, no `slice`, no `contains`, no character-level
predicates. The immediate driver is parsing single-string fields that carry
structure — a container image ref like `"registry/name:tag"` — but the goal is
general `elm/core` `String` parity, so authors reach for the same vocabulary
they know from Elm rather than a golem-specific dialect.

`elm/core`'s `String` module is built on **`Char`**: `toList : String -> List
Char`, `foldl : (Char -> b -> b) -> b -> String -> b`, `map : (Char -> Char) ->
String -> String`, `cons : Char -> String -> String`, and a whole `Char` module
of predicates (`isDigit`, `isAlpha`, …). Staying faithful to that API — the
decision Dr. Dub has already made — means Emet must grow a **`Char` primitive
type**. The String-only shortcut (expose only functions whose signatures never
mention `Char`) was considered and rejected: it would diverge from Elm exactly
where parsing code lives, and it cannot express `toList`/`map`/`foldl`, the
functions that make character-level work ergonomic.

Ground truth confirmed against the current tree:

- **No `Char` type exists.** `infer::builtin_type_arity` and
  `infer::builtin_types` list `String`/`Int`/`Float`/`Bool`/`Order`/… but no
  `Char`; the `Value` enum (`eval.rs`) has `Str`/`Int`/`Float`/… but no `Char`.
- **`String.length` counts Unicode scalars** — `string_length` is
  `as_string(..).chars().count()`. Every index this ADR introduces must agree
  with that.
- **`comparable` admits `Int | Float | String`** (`infer.rs`, the
  `Constraint::Comparable` arm of `constraint_admits`). `Char` is *not* there
  today. Elm's `Char` **is** comparable.
- **Emet has no tuple type.** `ast::Type` is `Var | Rigid | Con | Fun | Record`
  — no product type. This blocks any Elm function that returns a tuple, chiefly
  `String.uncons : String -> Maybe (Char, String)`.
- **`toInt`/`toFloat` already return `Maybe`** — the totality convention: a
  partial operation returns `Maybe`, never traps.
- **The lexer has no char literal.** `'` is not special; string escapes are
  `\n \t \" \\ \${`, with every *other* `\x` passing the raw `x` through
  (`scan_string_segment`). There is no `\u{...}` escape yet.

This ADR specifies the type, the literal syntax, and the two Elm-faithful
modules. It is a design record only — implementation is a following pass.

## Decision

### 1. The `Char` primitive type

Add `Char` as a new nullary base type, a peer of `Int`/`Float`/`String`. A
`Char` value is **exactly one Unicode scalar** — a Rust `char` — so it composes
cleanly with `String.length`'s scalar counting and with `chars()`-based
iteration.

- **Type registration:** add `"Char"` to `builtin_type_arity` (arity 0) and to
  `infer::builtin_types`, so signatures and patterns may name it.
- **Runtime value:** add `Value::Char(char)` to the `eval::Value` enum, with the
  obvious `Debug` arm.
- **Comparable and orderable (Elm-faithful):** add `"Char"` to the
  `Constraint::Comparable` admissible set and a `(Value::Char, Value::Char)` arm
  to `compare_values` (ordinary `char` ordering = codepoint order). This makes
  `==`, `/=`, `<`, `compare`, `min`, `max`, `List.sort`-style uses work on
  `Char` exactly as they do in Elm. `Char` is **not** `number` and **not**
  `appendable`.

**Char literal syntax `'c'`.** Add a lexer branch on `'` (before the operator
and identifier branches; note `'` already appears mid-identifier as a prime in
`is_ident_cont`, but a *leading* `'` is currently unreachable, so the branch is
unambiguous). A char literal is a single `'`, then either one non-`'`,
non-`\`, non-newline scalar or one escape, then a closing `'`. It lexes to a new
`Tok::Char(char)`. Escapes inside a char literal:

| literal  | scalar         |
|----------|----------------|
| `'\n'`   | newline        |
| `'\t'`   | tab            |
| `'\\'`   | backslash      |
| `'\''`   | single quote   |
| `'\u{1F600}'` | the codepoint |

An empty `''`, a multi-scalar `'ab'`, an unterminated `'a`, a raw newline, or a
bad `\u{...}` (empty, non-hex, or a value that is not a valid scalar — i.e. out
of range or a surrogate) is a **lex error**, consistent with how the string
scanner reports `unterminated string literal`.

**`\u{...}` in string literals too.** For consistency (Elm accepts `\u{HHHH}`
in both string and char literals), extend `scan_string_segment` to recognize
`\u{...}` and push the decoded scalar. This is a small, faithful upgrade;
because unknown `\x` currently passes `x` through, no existing valid program
changes meaning (no program relies on `\u` meaning literal `u`). `\u{...}` in a
string with an invalid codepoint is a lex error, same as in a char literal.

Parser/infer/eval wiring for the literal: `parser.rs` produces an `Expr::Char`
atom from `Tok::Char`; `infer.rs` gives `Expr::Char` the type `Con("Char", [])`
(mirroring `Expr::Str => con("String")`); `eval.rs` evaluates it to
`Value::Char`. Char **patterns** (`'c'` in a `case`) are deferred — not needed
by the driver, and out of scope here (a note, not a promise).

### 2. The `Char` module (Elm-faithful)

All total. `fromCode` follows Elm: an out-of-range or surrogate code yields the
Unicode replacement character `U+FFFD`, so it stays total without a `Maybe`.
The predicates are the ASCII-oriented `elm/core` definitions.

| function        | signature               | semantics |
|-----------------|-------------------------|-----------|
| `Char.toCode`   | `Char -> Int`           | Unicode codepoint. |
| `Char.fromCode` | `Int -> Char`           | Total; invalid/surrogate/out-of-range → `U+FFFD`. |
| `Char.toUpper`  | `Char -> Char`          | Unicode simple uppercase. |
| `Char.toLower`  | `Char -> Char`          | Unicode simple lowercase. |
| `Char.isUpper`  | `Char -> Bool`          | `'A'..'Z'` (Elm's ASCII definition). |
| `Char.isLower`  | `Char -> Bool`          | `'a'..'z'`. |
| `Char.isAlpha`  | `Char -> Bool`          | `isUpper \|\| isLower`. |
| `Char.isAlphaNum` | `Char -> Bool`        | `isAlpha \|\| isDigit`. |
| `Char.isDigit`  | `Char -> Bool`          | `'0'..'9'`. |
| `Char.isOctDigit` | `Char -> Bool`        | `'0'..'7'`. |
| `Char.isHexDigit` | `Char -> Bool`        | `'0'..'9'`, `'a'..'f'`, `'A'..'F'`. |
| `Char.isSpace`  | `Char -> Bool`          | space, `\t`, `\n`, `\r`, `\u{000B}`, `\u{000C}` (Elm's set). |

`toLocaleUpper`/`toLocaleLower` are **skipped** — locale is out of scope for a
config language, and Emet has no locale to key on.

### 3. The `String` module (Elm-faithful)

Elm signatures, Elm argument order, Elm total/clamping semantics. **Every index
is a Unicode scalar index** (see §5). Functions already in the prelude are
marked *exists* and are **not** redefined.

| function          | signature                                   | notes |
|-------------------|---------------------------------------------|-------|
| `String.isEmpty`  | `String -> Bool`                            | `== ""`. |
| `String.reverse`  | `String -> String`                          | by scalar. |
| `String.repeat`   | `Int -> String -> String`                   | `n <= 0` → `""`. |
| `String.replace`  | `String -> String -> String -> String`      | `before -> after -> str`; all occurrences. |
| `String.split`    | `String -> String -> List String`           | `sep -> str`; empty `sep` → list of single-scalar strings (Elm behavior). |
| `String.words`    | `String -> List String`                     | split on runs of whitespace, drop empties. |
| `String.lines`    | `String -> List String`                     | split on `\n` (and `\r\n`), Elm-style. |
| `String.slice`    | `Int -> Int -> String -> String`            | `start -> end -> str`; negatives count from end; clamped; empty if crossed. |
| `String.left`     | `Int -> String -> String`                   | first `n` scalars; total/clamped. |
| `String.right`    | `Int -> String -> String`                   | last `n` scalars; total/clamped. |
| `String.dropLeft` | `Int -> String -> String`                   | drop first `n`; clamped. |
| `String.dropRight`| `Int -> String -> String`                   | drop last `n`; clamped. |
| `String.contains` | `String -> String -> Bool`                  | `needle -> haystack`. |
| `String.startsWith` | `String -> String -> Bool`                | `prefix -> str`. |
| `String.endsWith` | `String -> String -> Bool`                  | `suffix -> str`. |
| `String.indexes`  | `String -> String -> List Int`              | `needle -> str`; scalar indices of all matches. |
| `String.indices`  | `String -> String -> List Int`              | Elm alias of `indexes`. |
| `String.toList`   | `String -> List Char`                       | |
| `String.fromList` | `List Char -> String`                       | |
| `String.fromChar` | `Char -> String`                            | |
| `String.cons`     | `Char -> String -> String`                  | prepend a `Char`. |
| `String.toUpper`  | `String -> String`                          | |
| `String.toLower`  | `String -> String`                          | |
| `String.trim`     | `String -> String`                          | both ends. |
| `String.trimLeft` | `String -> String`                          | |
| `String.trimRight`| `String -> String`                          | |
| `String.pad`      | `Int -> Char -> String -> String`           | center-pad to width `n`. |
| `String.padLeft`  | `Int -> Char -> String -> String`           | |
| `String.padRight` | `Int -> Char -> String -> String`           | |
| `String.map`      | `(Char -> Char) -> String -> String`        | |
| `String.filter`   | `(Char -> Bool) -> String -> String`        | |
| `String.foldl`    | `(Char -> b -> b) -> b -> String -> b`      | left fold over scalars. |
| `String.foldr`    | `(Char -> b -> b) -> b -> String -> b`      | right fold. |
| `String.any`      | `(Char -> Bool) -> String -> Bool`          | |
| `String.all`      | `(Char -> Bool) -> String -> Bool`          | |

Already present, **not** touched by this ADR: `String.append`, `String.concat`,
`String.join`, `String.length`, `String.fromInt`, `String.fromFloat`,
`String.toInt`, `String.toFloat` (and the polymorphic `++`/`append`).

### 4. The tuple wrinkle — `uncons` is deferred (decided)

Elm's `String.uncons : String -> Maybe (Char, String)` returns a tuple, and
Emet **has no tuple type**. The same wall blocks any future tuple-returning
neighbor. **Decision: `uncons` — and every tuple-returning function — is
deferred until Emet has a tuple type. We do not deviate to a record.**

The rejected alternative was `String.uncons : String -> Maybe { head : Char,
tail : String }`. It is expressible today, but it is a **deviation from Elm's
path** — an author who knows `elm/core` reaches for `Maybe (Char, String)` from
muscle memory and would be surprised by a record; and it bakes a shape that a
later real tuple type would then compete with. It buys one function at the cost
of the "faithful to `elm/core`" property this whole ADR exists to preserve. That
trade is not worth it: everything else in the module is expressible today,
`uncons` is the lone hold-out, and it is not needed by the driver (splitting a
ref uses `split`/`slice`/`indexes`).

The real fix is a **separate, future "tuple type" ADR** (adding a tuple type is
far larger than this record — type syntax, inference, pattern matching, IR
implications — and is the wrong thing to settle here). Once tuples land,
`uncons` is added with its exact Elm signature and nothing in this ADR needs
revisiting. The deferral is logged in `docs/TODO.md`.

### 5. Index and Unicode semantics (stated once)

**Every length, index, and slice bound in both modules is measured in Unicode
scalar values (Rust `char`), not UTF-8 bytes and not grapheme clusters.** This
is the exact model of the existing `String.length` (`chars().count()`), and a
`Char` *is* one scalar. So `String.length s == List.length (String.toList s)`,
`String.slice`/`left`/`right`/`indexes` all count in the same unit, and the two
modules are mutually consistent by construction. Combining characters and
emoji-with-modifiers therefore count as multiple scalars — the same caveat Elm
carries, accepted here for the same reason.

### 6. Implementation surface (plan, not code)

- **`lexer.rs`** — add `Tok::Char(char)`; a `'…'` scanning branch with the four
  escapes plus `\u{...}`; teach `scan_string_segment` the `\u{...}` escape;
  `Display` arm for `Tok::Char`.
- **`ast.rs`** — `Expr::Char(char)` atom.
- **`parser.rs`** — `parse_atom` produces `Expr::Char` from `Tok::Char`.
- **`infer.rs`** — `"Char"` in `builtin_type_arity` (0) and `builtin_types`;
  `Expr::Char => con("Char")`; `"Char"` added to the `Comparable` admissible
  set.
- **`eval.rs`** — `Value::Char(char)` + `Debug`; `Expr::Char` evaluates to it;
  `compare_values` gains a `Char` arm; an `as_char` accessor beside `as_string`.
- **`prelude.rs`** — a `char()` type-builder beside `string()`/`int()`; the
  `Char.*` builtins (§2) and the new `String.*` builtins (§3), each with its
  Elm-accurate scheme, registered in `builtins()` so `ty_env`/`env` pick them up
  in lockstep.
- **Tests** — one Elm-parity example per function (e.g. `String.slice 7 9
  "snakes on a plane!" == "on"`, `String.slice -6 -1 "snakes…" == "plane"`,
  `Char.isDigit '0' == True`, `Char.fromCode 0x1F600 |> Char.toCode == 128512`,
  `Char.fromCode 0xD800 == '\u{FFFD}'`, `String.toList "abc" == ['a','b','c']`),
  plus lexer tests for each escape and each rejection case, plus a
  round-trip `String.fromList (String.toList s) == s`.

### 7. Scope call

Build the **full faithful Char + String core in one pass** (§2 and §3,
including `pad`/`padLeft`/`padRight`) — the surface is a flat table of
independent, individually-testable builtins, so there is no staging benefit to
splitting it, and a partial surface is the thing an Elm author trips on.
**Explicitly out of scope:** `toLocaleUpper`/`toLocaleLower` (locale),
`String.uncons` (tuple, §4), and `Char`/`String`-literal *patterns* in `case`.

## Consequences

- **A fifth primitive type joins the language.** `Char` sits beside
  `Int`/`Float`/`String`/`Bool` in inference and evaluation. It is small (one
  Rust `char`) and total, but it is a real base type: every place that
  enumerates base types (`builtin_type_arity`, `builtin_types`, `compare_values`,
  the `comparable` set) grows a `Char` case, and future value-level machinery
  (equality, hashing, any serialization of `Value`) must handle it. `Char` never
  reaches the wire — it is an authoring-time value only; glyph fields stay
  `String` (no golemd/`scroll-format` change).
- **`'` becomes significant at the start of a token.** It stays a prime
  mid-identifier (`x'`), but a leading `'` now opens a char literal. This is a
  lexer-only change and does not touch layout.
- **String work becomes Elm.** Parsing a `"registry/name:tag"` ref, and general
  string manipulation, use the same names and argument orders as `elm/core`, so
  the mental model transfers with no golem-specific relearning.
- **The tuple gap is now visible and logged.** Deferring `uncons` (a settled
  decision, not an open item) records the absence of a tuple type as the actual
  missing feature and points at a future ADR, rather than papering over it with
  a record that would quietly diverge from Elm. Tracked in `docs/TODO.md`.
- **One consistent index unit, forever.** Fixing "everything counts in scalars"
  now, matching the existing `String.length`, prevents a later byte-vs-scalar
  vs-grapheme split across the two modules. It inherits Elm's grapheme caveat.
- **Cross-references:** extends the prelude builtin set and totality convention
  of ADR 0006/0007; unaffected by and unaffecting the module system (ADR
  0016/0024) and the binary manifest (ADR 0012/0013). A future "tuple type" ADR
  is the prerequisite for `String.uncons` and any tuple-returning addition.
