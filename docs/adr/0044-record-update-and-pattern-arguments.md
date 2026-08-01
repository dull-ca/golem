# 0044 — Record update and constructor patterns in arguments

## Status

Proposed 2026-08-01. Extends ADR 0010 (row-polymorphic records) and ADR
0005/0017 (patterns); both features are Elm's, already specified by it, and
are adopted verbatim rather than designed here.

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
base unifies with an open record demanding the named fields, and the result
is the base's type with those fields' types substituted. Updating a field
the record does not have is a type error naming the field. As in Elm, the
base is an arbitrary expression, not only a variable.

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
- A module can expose `Config(..)` plus a `defaults` value and drop its
  `with*` family entirely: callers state the overrides at the construction
  site. That is the shape library authors keep reaching for.
- Emet moves closer to Elm rather than further away — both features are
  ones an Elm programmer already expects, so the surface gets smaller to
  learn, not larger.
- Record update makes closed-record types slightly more load-bearing:
  `{ r | typo = v }` is a type error, which is the point, but the error
  must name the field and list the record's fields to be useful.
- Constructor patterns in arguments are restricted to single-constructor
  types; extending them to refutable positions would silently reintroduce
  partial functions, which ADR 0005 exists to prevent.
