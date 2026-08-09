# How-to

Recipes. Each assumes the [tutorial](tutorial.md). Run any example with
`cargo run -p emet -- build --text apps/emet/examples/<file>.emet`.

## Generate a fleet from a list

Write a `... -> Scroll` helper and map it. Over names:

```elm
main : List Scroll
main = List.map webHost [ "web-1", "web-2", "web-3" ]
```

Over numbers, map over `List.range`:

```elm
node : Int -> Scroll
node i = scroll { name = "node-${String.fromInt i}", glyphs = [ aptPackage { name = "consul" } ] }

main : List Scroll
main = List.map node (List.range 1 3)
```

Full: [`apps/emet/examples/fleet.emet`](../../apps/emet/examples/fleet.emet).

## Read fields off a record parameter

Take the per-host data as a record and read its fields — `h.name`, `h.port` —
including inside a lambda passed to `List.map`:

```elm
main : List String
main = List.map (\h -> h.name) [ { name = "a", port = 1 }, { name = "b", port = 2 } ]
```

Full host example: [`apps/emet/examples/record-hosts.emet`](../../apps/emet/examples/record-hosts.emet).

## Write a config file from computed values

Build the `contents` string in the language, then wrap it in `file`. `String.join
"\n"` for multiple lines; `String.fromInt` to embed a number.

```elm
renderConfig : Int -> String
renderConfig port =
  String.join "\n"
    [ "[server]"
    , "listen = ${String.fromInt port}"
    , "workers = 4"
    ]
```

`emetc` evaluates the interpolation where you compile it, so the glyph receives
the finished string. Full:
[`apps/emet/examples/config-file.emet`](../../apps/emet/examples/config-file.emet).

## Make a field optional with `Maybe`

Take the setting as a `Maybe` and resolve it with `Maybe.withDefault` at use.

```elm
hostScroll : String -> Maybe Int -> Scroll
hostScroll name port =
  scroll
    { name = name
    , glyphs =
        [ file
            { path = "/etc/app/port.conf"
            , contents = "listen = ${String.fromInt (Maybe.withDefault 8080 port)}"
            , mode = "0644"
            }
        ]
    }

main : List Scroll
main =
  [ hostScroll "app-1" (Just 9090)
  , hostScroll "app-2" Nothing
  ]
```

`app-2` has no override, so it falls back to 8080. Full:
[`apps/emet/examples/optional-port.emet`](../../apps/emet/examples/optional-port.emet).

## Branch on a role with your own type

Declare a `type`, then `case` on it. Exhaustiveness is checked — a missing arm is
a compile error.

```elm
type Role = Web | Db

glyphsFor : Role -> List Glyph
glyphsFor role =
  case role of
    Web -> [ aptPackage { name = "nginx" } ]
    Db  -> [ aptPackage { name = "postgresql" } ]
```

Full: [`apps/emet/examples/roles.emet`](../../apps/emet/examples/roles.emet).

## Build values with recursion

When a built-in combinator does not fit, write the recursion yourself. Here a
helper builds host records without `List.range`:

```elm
nodes : Int -> List { name : String }
nodes n =
  if n <= 0 then
    []
  else
    List.append (nodes (n - 1)) [ { name = "node-${String.fromInt n}" } ]
```

Full: [`apps/emet/examples/numbered-nodes.emet`](../../apps/emet/examples/numbered-nodes.emet). A
declaration may call itself, and two declarations may call each other; Emet does
not check that your recursion terminates.

## Build a heterogeneous fleet

Different roles, different glyphs. One helper per role, joined with `List.append`:

```elm
main : List Scroll
main =
  List.append
    (List.map webHost [ "web-1", "web-2" ])
    [ dbHost "db-1" ]
```

Full: [`apps/emet/examples/heterogeneous-fleet.emet`](../../apps/emet/examples/heterogeneous-fleet.emet).

## Share a package across hosts without a conflict

Conflict detection is scoped to one leaf unit, so two scrolls — or two leaves of
one scroll — may install the same package:

```elm
main : List Scroll
main =
  [ scroll { name = "a", glyphs = [ aptPackage { name = "curl" } ] }
  , scroll { name = "b", glyphs = [ aptPackage { name = "curl" } ] }
  ]
```

A conflict is reported when one leaf declares the same resource key two different
ways — two `file` glyphs at one path with different contents, say. Two identical
glyphs in one leaf are fine.
