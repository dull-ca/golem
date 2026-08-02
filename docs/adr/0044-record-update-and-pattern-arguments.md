# 0044 — Record update and constructor patterns in arguments

## Status

Accepted 2026-08-01. Extends ADR 0010 (row-polymorphic records) and ADR
0005/0017 (patterns). Both features are Elm's and are adopted with its syntax
and its typing rules rather than designed here; the two places the surface
diverges from Elm are stated below and in the consequences.

Implemented on `lakin/feat-record-updates`: `79d827b` record update,
`9890e45` constructor patterns in argument position, `17c8da0` record update
made type-preserving with field-name spans, `c34d253` the refutability gate
made exhaustive and recursive. `fd708ac` collapses
`examples/limesurvey/Limesurvey.emet` — the motivating example below — onto
both features, with a byte-identical manifest.

## Context

Writing a configurable library in Emet costs a rebuild of every field. The
LimeSurvey example's `Config` has five fields, so each setter restates all
five and unwraps the constructor by hand:

```text
withAdministrator name c =
  case c of
    Config spec ->
      Config
        { domain = spec.domain
        , administrator = name
        , administratorEmail = spec.administratorEmail
        , administratorPassword = spec.administratorPassword
        , databasePassword = spec.databasePassword
        }
```

Four of those five lines are noise, and the cost is quadratic in the wrong
direction: adding a field edits every setter, and forgetting one is only
caught because record construction is total — a silent wrong value is not
possible, but the edit is unavoidable. The pattern recurs anywhere a module
offers defaults a caller overrides, which is the shape ADR 0023's library
layer pushes authors toward.

Emet is missing two Elm features that together remove it. Both were
omissions rather than decisions: ADR 0010 built row polymorphism and stopped
at field access; patterns were specified for `case` and never extended to
binding positions.

## Decision

Adopt both, with Elm's syntax and Elm's semantics.

**Record update.** `{ r | field = value, … }` produces a copy of `r` with
the named fields replaced. Typing follows directly from ADR 0010's rows: the
base unifies with an open record demanding the named fields, so a base that
is already open stays open and a setter written against a row-polymorphic
parameter serves every record shape carrying those fields.

The rule is **type-preserving**. Each new value must have the type the field
already has, and the result type is the base's type, unchanged. An update
changes what a record holds and never its shape; changing the shape is what a
record literal is for. Updating a field the record does not have is a type
error naming the field.

The base may be an arbitrary expression, not only a variable. This is a
deliberate **superset** of Elm 0.19, which requires a lowercase variable
after `{`. Nothing in the row machinery needs the restriction, so imposing it
would be a rule to remember for no gain.

**Constructor patterns in argument position.** A function or lambda
parameter may be a constructor pattern — `f (Box spec) = …` — for a
single-constructor type. Multi-constructor types stay `case`-only, so
exhaustiveness (ADR 0005) is never bypassed: a pattern that cannot fail is
allowed to bind, one that can must branch.

Deliberately **not** adopted:

- **Scala's `.copy(field = v)`** — needs named arguments, which Emet has
  nowhere else, and named arguments in a curried language are a separate
  design. Record update reaches the same ergonomics without touching call
  syntax.
- **Rust's `{ ..base, field = v }`** — same semantics as record update in
  different clothes; two spellings of one idea is worse than one.
- **Update through an opaque wrapper** (`{ c | field = v }` where `c` is a
  custom type, unwrapping implicitly). It reads well and would shorten the
  above further, but it is genuine invention with no reference
  implementation, and it makes a constructor's visibility silently
  load-bearing. Constructor patterns get most of the benefit at none of
  that cost. Revisit only if the remaining ceremony proves to hurt.

## Consequences

- A setter becomes one line, and adding a field to a config type stops
  editing every function that touches it.
- A module whose defaults are **constants** can expose `Config(..)` plus a
  `defaults` value and drop its `with*` family entirely: callers state the
  overrides at the construction site.

  The condition is load-bearing. Where a default is *computed* from another
  field, `defaults` would have to be a function of that field — which is the
  smart constructor the module already has. `Limesurvey.emet` is that case:
  its `administratorEmail` defaults to `admin@${domain}`, so it keeps both
  `config` and its `with*` family. What it gains is that each setter collapses
  to one line —
  `withAdministrator name (Config spec) = Config { spec | administrator = name }`
  — rather than that the family disappears.
- Emet moves closer to Elm rather than further away — both features are
  ones an Elm programmer already expects, so the surface gets smaller to
  learn, not larger. It diverges in exactly two places, both recorded here:
  the base of an update may be any expression, and a nullary constructor
  parameter needs parentheses.
- Record update makes closed-record types slightly more load-bearing:
  `{ r | typo = v }` is a type error, which is the point, but the error
  must name the field and list the record's fields to be useful.
- Constructor patterns in arguments are restricted to single-constructor
  types; extending them to refutable positions would silently reintroduce
  partial functions, which ADR 0005 exists to prevent.
- A nullary constructor keeps its parentheses in argument position:
  `f (Unit) = …` binds, `f Unit = …` is a parse error, where Elm accepts the
  bare form. That is the price of a parameter grammar admitting only a binder
  or a parenthesized constructor application, and the narrowness is what makes
  the restriction airtight — the refutable spellings (`f []`, `f "x"`, `f 0`)
  cannot be written at all, so the type-level gate is a second line of defence
  rather than the only one. Cheap to relax if the divergence bites.
