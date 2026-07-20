# Tutorial: build a fleet

Write each program into a file and run it with `cargo run -- file.emet`. Every
step compiles.

## One host

`main` is always `List Scroll`. A `Scroll` is one host's desired state: a `name`
and a list of `glyphs`. A glyph is one resource; `aptPackage` installs a package.

```elm
main : List Scroll
main =
  [ scroll
      { name = "web"
      , glyphs = [ aptPackage { name = "nginx" } ]
      }
  ]
```

```
planned scrolls (1):
  scroll `web` (1 glyphs):
    * ensure apt package `nginx` installed
```

Add more glyphs to the list; `systemdService` enables a unit, order is kept:

```elm
main : List Scroll
main =
  [ scroll
      { name = "web"
      , glyphs =
          [ aptPackage { name = "nginx" }
          , systemdService { unit = "nginx.service" }
          ]
      }
  ]
```

## A recipe as a function

Repeating that block per host is waste. Pull it into a function. A declaration is
`name args = body`; the line above is an optional signature.

```elm
webHost : String -> Scroll
webHost name =
  scroll
    { name = name
    , glyphs =
        [ aptPackage { name = "nginx" }
        , systemdService { unit = "nginx.service" }
        ]
    }

main : List Scroll
main = [ webHost "web-1" ]
```

Application is juxtaposition — `webHost "web-1"`, no parentheses.

## Many hosts with `List.map`

`List.map` applies a function to every element. Give it `webHost` and a list of
names to get a `List Scroll`:

```elm
main : List Scroll
main = List.map webHost [ "web-1", "web-2", "web-3" ]
```

This is [`examples/fleet.emet`](../../examples/fleet.emet) — three scrolls from
one function.

## Per-host data with a record

Hosts usually differ by more than a name. Pass a record and read its fields; the
type checker fixes each field at the call site.

```elm
host : { name : String, port : Int } -> Scroll
host h =
  scroll
    { name = h.name
    , glyphs =
        [ aptPackage { name = "app" }
        , file
            { path = "/etc/app/port.conf"
            , contents = "listen = ${String.fromInt h.port}"
            , mode = "0644"
            }
        , systemdService { unit = "app.service" }
        ]
    }

main : List Scroll
main =
  List.map host
    [ { name = "web-1", port = 8080 }
    , { name = "web-2", port = 9090 }
    ]
```

This is [`examples/record-hosts.emet`](../../examples/record-hosts.emet). Two
things to note: `"listen = ${String.fromInt h.port}"` is string interpolation —
`${expr}` splices a `String` into the literal, so the plan carries
`listen = 8080`, never a placeholder. And `h.port` reads a field off the record
parameter directly.

## Roles with your own type

Declare a type when a host has a fixed set of kinds, and use `case` to branch.
`case` is checked for exhaustiveness — leave out `Db` and the program is rejected.

```elm
type Role = Web | Db

glyphsFor : Role -> List Glyph
glyphsFor role =
  case role of
    Web -> [ aptPackage { name = "nginx" }, systemdService { unit = "nginx.service" } ]
    Db  -> [ aptPackage { name = "postgresql" }, systemdService { unit = "postgresql.service" } ]

host : { name : String, role : Role } -> Scroll
host h = scroll { name = h.name, glyphs = glyphsFor h.role }

main : List Scroll
main =
  List.map host
    [ { name = "web-1", role = Web }
    , { name = "db-1", role = Db }
    ]
```

This is [`examples/roles.emet`](../../examples/roles.emet).

## Next

- [How-to](how-to.md) — optional fields, heterogeneous fleets, recursion,
  avoiding conflicts.
- [Reference](reference.md) — every glyph, type, and prelude function.
