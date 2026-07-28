# Reference

## Output

`main : List Scroll` is required — the fleet, one scroll per host. A bare `Scroll`
is rejected; wrap a single host in a one-element list.

## Glyphs

One glyph is one OS resource. Four reserved constructors build them; each takes a
record whose fields are all `String`. The IR key is used for per-scroll conflict
detection.

| Constructor | Fields | Type | IR key |
|---|---|---|---|
| `aptPackage` | `name` | `AptPackage` | `apt:<name>` |
| `systemdService` | `unit` | `SystemdService` | `systemd:<unit>` |
| `file` | `path`, `contents`, `mode` | `File` | `file:<path>` |
| `lineInFile` | `path`, `line` | `LineInFile` | `fileline:<path>:<line>` |

```elm
aptPackage     { name = "nginx" }
systemdService { unit = "nginx.service" }
file           { path = "/etc/x", contents = "…", mode = "0644" }
lineInFile     { path = "/etc/hosts", line = "10.0.0.1 db" }
```

Each injects into `Glyph`, so glyphs of any kind mix in one list.

## Scroll

```elm
scroll { name = String, glyphs = List Glyph } : Scroll
```

Within one scroll, two glyphs sharing an IR key must be identical or the program
is rejected. Across scrolls the same key is allowed.

## Types

| Type | Notes |
|---|---|
| `String` | `Str` is a transitional alias. |
| `Int`, `Float` | `3.0` is `Float`. |
| `Bool` | `True` \| `False`. |
| `Order` | `LT` \| `EQ` \| `GT`. |
| `Maybe a` | `Just a` \| `Nothing`. |
| `List a` | literal `[a, b, c]`. |
| records | `{ field : T }`; row-polymorphic (see below). |
| `AptPackage`, `SystemdService`, `File`, `LineInFile`, `Glyph`, `Scroll` | resource types. |

`Just`, `Nothing`, `True`, `False`, `LT`, `EQ`, `GT` are constructor values usable
as functions (`Just : a -> Maybe a`).

### User types

```elm
type Status = Up | Down
type Tree a = Leaf | Node (Tree a) a (Tree a)
```

Constructors are values; `Node : Tree a -> a -> Tree a -> Tree a`. Types may be
recursive.

## Prelude

Qualified names (`List.map`) are a naming convention, not a module system. Numeric
and comparison functions are bare, as in Elm.

### List

```
List.map        : (a -> b) -> List a -> List b
List.filter     : (a -> Bool) -> List a -> List a
List.foldr      : (a -> b -> b) -> b -> List a -> b
List.foldl      : (a -> b -> b) -> b -> List a -> b
List.concat     : List (List a) -> List a
List.concatMap  : (a -> List b) -> List a -> List b
List.append     : List a -> List a -> List a
List.isEmpty    : List a -> Bool
List.length     : List a -> Int
List.range      : Int -> Int -> List Int
List.sum        : List number -> number
```

### Maybe

```
Maybe.map         : (a -> b) -> Maybe a -> Maybe b
Maybe.withDefault : a -> Maybe a -> a
Maybe.andThen     : (a -> Maybe b) -> Maybe a -> Maybe b
```

### String

```
String.append   : String -> String -> String
String.concat   : List String -> String
String.join     : String -> List String -> String
String.length   : String -> Int
String.fromInt  : Int -> String
String.fromFloat: Float -> String
String.toInt    : String -> Maybe Int
String.toFloat  : String -> Maybe Float
```

### Numeric and comparison (bare)

```
toFloat     : Int -> Float
round       : Float -> Int
floor       : Float -> Int
ceiling     : Float -> Int
truncate    : Float -> Int
negate      : number -> number
abs         : number -> number
clamp       : number -> number -> number -> number
modBy       : Int -> Int -> Int
remainderBy : Int -> Int -> Int
min         : comparable -> comparable -> comparable
max         : comparable -> comparable -> comparable
compare     : comparable -> comparable -> Order
not         : Bool -> Bool
```

## Operators

Every operator desugars to a prelude function.

| Prec | Operators | Assoc | Meaning |
|---|---|---|---|
| 7 | `^` | right | power |
| 7 | `*` `/` `//` | left | multiply, float-divide, integer-divide |
| 6 | `+` `-` | left | add, subtract |
| 5 | `++` | right | append (`String` or `List`) |
| 4 | `==` `/=` `<` `>` `<=` `>=` | non-assoc | equality, comparison |
| 3 | `&&` | right | and |
| 2 | `\|\|` | right | or |

`/` is float division, `//` integer. Level 4 is non-associative, so `a < b < c`
is a parse error. Division and `modBy` / `remainderBy` by zero return `0` rather
than trapping.

## Bounded type variables

Two closed constraints, as in Elm; no user typeclasses.

- `number` — `Int` or `Float`. An integer literal defaults to `Int` if nothing
  forces `Float`.
- `comparable` — `Int`, `Float`, or `String`.

## Syntax

**Declarations.** `name args = body`, separated by layout. An optional signature
sits on the line above and is checked. A declaration may call itself; it may not
reference a later declaration (no mutual recursion).

**Signatures and generics.** Hindley-Milner with let-generalization:
`map : (a -> b) -> List a -> List b`.

**Records.** `{ a = e }`, field access `e.a`. Inference is row-polymorphic, so
`\h -> h.name` and helpers that read fields off a record parameter type-check; the
record only needs the fields it uses.

**`let … in`.** Local bindings, layout-driven; single-line `let x = e in e` works.

```elm
main : List Scroll
main =
  let u = "redis.service"
  in [ scroll { name = "cache", glyphs = [ systemdService { unit = u } ] } ]
```

**`case … of`.** Exhaustiveness- and redundancy-checked. Arms are laid out, one
per line.

```elm
describe : Maybe Int -> String
describe m =
  case m of
    Just n  -> "port ${String.fromInt n}"
    Nothing -> "default"
```

**`if … then … else`.** Sugar for `case c of True -> a ; False -> b`.

**Lambdas.** `\x y -> e`. Application is juxtaposition: `f x`.

**String interpolation.** `"port ${expr}"`, where `expr` is `String`-typed.
Desugars to `String.concat`; combine with `String.fromInt` to embed a number.

**Comments.** `--` to end of line.
