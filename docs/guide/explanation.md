# Explanation

## Glyphs, scrolls, fleet

Emet describes desired state at two levels. A **glyph** is one resource: install
this package, enable this unit, ensure this file. A **scroll** is one host's
desired state under a name: a tree whose leaves each hold a list of glyphs, and
whose branches group those leaves by subsystem. The program's output is
`main : List Scroll` — the fleet.

The names follow the golem legend: a golem is animated by the glyphs inscribed on
it; a scroll is one host's full set of marks; the fleet is the complete desired
state.

A leaf is the scope of conflict detection. The same package on two hosts is
fine — both web servers install `nginx` — and so is the same package in two
leaves of one host. What Emet rejects is two *different* declarations of the same
resource key inside a single leaf: one unit contradicting itself. Keeping the
scope that tight lets a fleet share resources freely while still catching that
(ADR 0031).

## Emet writes; golemd enacts

Emet produces the fleet and stops. A separate daemon, `golemd` (part of the golem
ecosystem), reconciles that fleet against real hosts. The language does not run
or reconcile anything.

## Evaluation happens on your machine

Interpolation like `"listen = ${String.fromInt port}"` is computed by `emetc`,
where you compile, and the glyph receives the finished value. The same goes for
everything else you would otherwise push into a template — conditionals, string
building, defaults, generated host lists: you write it in the language, where the
type checker sees it. What arrives at a host is glyphs and scrolls that have
already type-checked and already run.

The one value a host resolves for itself is a **secret**. A value-bearing field
is a `Text`: either a plain string, or a run of literal chunks with sealed holes
in it (ADR 0047). `emetc` fetches the secret through secretspec at compile time
and encrypts it to the fleet key; `golemd` unseals the hole and joins the pieces
as it writes the file. The text around the hole stays readable in the manifest
and in `golemctl plan`, and unsealing is the whole of what the host computes.

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
- `docs/adr/` — decisions: glyph primitives (0002), client-side evaluation and
  string interpolation (0004), `case` and exhaustiveness (0005), numbers and
  operators (0007), the `Scroll` container (0009), row-polymorphic records
  (0010), recursion (0011), the recursive scroll and failure isolation (0031),
  typed secrets on the wire (0047).
