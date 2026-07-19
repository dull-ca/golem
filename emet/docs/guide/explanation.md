# Explanation

## Glyphs, scrolls, fleet

Emet describes desired state at two levels. A **glyph** is one resource: install
this package, enable this unit, ensure this file. A **scroll** groups one host's
glyphs under a name. The program's output is `main : List Scroll` — the fleet.

The names follow the golem legend: a golem is animated by the glyphs inscribed on
it; a scroll is one host's full set of marks; the fleet is the complete desired
state.

The split defines what counts as a conflict. The same package on two hosts is
fine — both web servers install `nginx`. A conflict is two *different*
declarations of the same resource key *within one scroll*: a host contradicting
itself. Making the scroll the unit lets a fleet share resources freely while still
catching that.

## Emet writes; golemd enacts

Emet produces the fleet and stops. The list of scrolls is inert, fully-evaluated
data — every string already computed, no placeholder left. A separate daemon,
`golemd` (part of the golem ecosystem), reconciles that fleet against real hosts.
The language does not run or reconcile anything.

## No templating

Most config tools emit a template that something else expands later. Emet does
not. Interpolation like `"listen = ${String.fromInt port}"` is evaluated to a
concrete string before it reaches a glyph, so a `file`'s `contents` is a finished
`String`, not a template with a hole. Any logic you would push into a template —
conditionals, string building, defaults — you write in the language, where the
type checker sees it.

## The type system

Hindley-Milner inference with generics and let-generalization, plus a few
deliberate extensions:

- **Row-polymorphic records** — `\h -> h.name` type-checks; a helper's record
  parameter only needs the fields it reads.
- **`number` / `comparable`** — two closed constraints, taken exactly as far as
  Elm and no further. `number` is `Int` or `Float`; `comparable` is `Int`,
  `Float`, or `String`. No user-defined typeclasses.
- **Exhaustive `case`** — a match must cover every constructor; a missing or
  redundant arm is a compile error.

Recursion is allowed — you can write recursive functions and recursive types — so
Emet does not guarantee termination.

## Further reading

- `docs/design/0001-…` — the full design.
- `docs/adr/` — decisions: glyph primitives (0002), no-templating (0004), `case`
  and exhaustiveness (0005), numbers and operators (0007), the `Scroll` container
  (0009), row-polymorphic records (0010), recursion (0011).
