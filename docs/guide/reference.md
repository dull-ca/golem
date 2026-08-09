# Reference

`emetc` runs a program to completion on the machine you compile on. Every value
below — an interpolated string, a parsed mode, a resolved secret — is finished
before it reaches the manifest.

## Output

`main : List Scroll` is required — the fleet, one scroll per host. A bare
`Scroll` is rejected with `` `main` must be `List Scroll` ``; wrap a single host
in a one-element list. Exactly one module declares `main`; every other module is
a library.

## Glyphs

One glyph is one OS resource. There are four kinds and six constructors that
build them: `file`, `directory`, and `symlink` are three spellings of the
filesystem glyph, differing in the entry they place at the path.

| Constructor | Fields | Type | Key |
|---|---|---|---|
| `aptPackage` | `name` | `AptPackage` | `apt:<name>` |
| `systemdService` | `unit` | `SystemdService` | `systemd:<unit>` |
| `file` | `path`, `contents`, `mode` | `Filesystem` | `file:<path>` |
| `directory` | `path`, `mode` | `Filesystem` | `file:<path>` |
| `symlink` | `path`, `target` | `Filesystem` | `file:<path>` |
| `lineInFile` | `path`, `line` | `LineInFile` | `fileline:<path>:<line>` |

Every field takes a `String`.

```elm fragment
aptPackage     { name = "nginx" }
systemdService { unit = "nginx.service" }
file           { path = "/etc/app.conf", contents = "listen = 8080", mode = "0644" }
directory      { path = "/var/lib/app", mode = "0755" }
symlink        { path = "/etc/app.conf.bak", target = "/etc/app.conf" }
lineInFile     { path = "/etc/hosts", line = "10.0.0.1 db" }
```

Each constructor takes exactly the fields listed and no others, so
`symlink { path = …, target = …, mode = "0644" }` is an unknown-field error: a
symlink with a mode cannot be written down.

The four glyph types widen into `Glyph` in one direction. Glyphs of any kind
collect into one `List Glyph`; a `Glyph` is not accepted where a concrete type
such as `AptPackage` is required.

Surface strings are lowered on the way to the manifest. `mode` is parsed as
octal — with or without a `0o` prefix — into the twelve permission bits, so a
non-octal mode, or one above `0o7777`, fails at compile time. `contents` and
`line` become a text value: plain text, or literal chunks around a sealed hole
(see [Secrets](#secrets)). The wire also carries `owner` and `group` beside the
mode; no surface constructor sets them, so authored entries leave ownership
unmanaged.

The ten lowercase words `aptPackage`, `systemdService`, `file`, `directory`,
`symlink`, `lineInFile`, `scroll`, `retry`, `rollback`, and `keep` are reserved
and cannot be bound as ordinary names.

## Scroll

```elm fragment
scroll
  { name : String
  , policy : Policy         -- optional
  , notifies : List String  -- optional
  , glyphs : List Glyph     -- exactly one of
  , groups : List Scroll    --   `glyphs` or `groups`
  } : Scroll
```

A scroll is one host's desired state, or one named part of it. A **leaf** holds
`glyphs`; a **branch** holds `groups` of sub-scrolls. Exactly one of the two is
written — writing both, or neither, is an error. `policy` and `notifies` are
optional at every level.

```elm fragment
scroll
  { name = "web"
  , notifies = [ "nginx.service" ]
  , groups =
      [ scroll { name = "packages", glyphs = [ aptPackage { name = "nginx" } ] }
      , scroll
          { name = "config"
          , policy = retry { maxAttempts = 3, baseDelayMs = 250, onExhaust = keep }
          , glyphs = [ file { path = "/etc/nginx/site.conf", contents = "listen 80;", mode = "0644" } ]
          }
      ]
  }
```

The leaf is the scope of conflict detection. Two glyphs in one leaf that share a
key must be identical; differing bodies are rejected at the second declaration.
Sibling leaves of one scroll, and separate scrolls, may share a key freely.

### Policy

`retry`, `rollback`, and `keep` all build a `Policy`. `rollback` and `keep` are
written without braces and choose only what happens when the retry budget runs
out; `rollback` is the default.

```elm fragment
policy = keep
policy = retry { maxAttempts = 5, backoffMultiplier = 2.0, onExhaust = rollback }
```

| Field | Type |
|---|---|
| `maxAttempts` | `Int` |
| `baseDelayMs` | `Int` |
| `backoffMultiplier` | `Float` |
| `maxDelayMs` | `Int` |
| `jitterFraction` | `Float` |
| `maxElapsedMs` | `Int` |
| `onExhaust` | `Policy` — `rollback` or `keep` |

Every field is optional; an absent one is inherited nearest-wins from the
enclosing scrolls, then from `golemd`'s own configuration. A branch's policy
cascades to the leaves beneath it.

### Notifies

`notifies` is a `List String` of systemd units to reload once anything in or
under the scroll lands changed. It unions downward rather than cascading: a
leaf's obligation is every `notifies` entry on its root-to-leaf path, in
first-mention order.

## Types

| Type | Notes |
|---|---|
| `Int`, `Float` | `3.0` is `Float`. An integer literal is a `number`, defaulting to `Int`. |
| `String` | `"…"`, with `${…}` interpolation. |
| `Char` | `'c'` — exactly one Unicode scalar. |
| `Bool` | `True` \| `False`. |
| `Order` | `LT` \| `EQ` \| `GT`. |
| `Maybe a` | `Just a` \| `Nothing`. |
| `List a` | literal `[a, b, c]`; `::` prepends. |
| tuples, unit | `(a, b)`, `(a, b, c)`, `()`. Four or more elements is an error — use a record. |
| records | `{ field : T }`; row-polymorphic (see [Syntax](#syntax)). |
| `AptPackage`, `SystemdService`, `Filesystem`, `LineInFile` | one per glyph kind; each widens into `Glyph`. |
| `Glyph` | the sum of the four, and the element type of a leaf's `glyphs`. |
| `Entry` | what a `Filesystem` glyph places at its path: `File` \| `Directory` \| `Symlink`. |
| `Scroll` | a node of the scroll tree. |
| `Contents` | a scroll's payload, `Glyphs` \| `Groups`. Written by choosing `glyphs` or `groups` on `scroll`; useful only in a signature. |
| `Policy`, `OnExhaust` | the retry knobs and the exhaustion decision. `rollback` and `keep` build a `Policy`, so `OnExhaust` has no surface spelling either. |

`Just`, `Nothing`, `True`, `False`, `LT`, `EQ`, `GT` are constructor values
usable as functions (`Just : a -> Maybe a`).

`Char`, tuples, and unit are authoring-time types. No glyph field holds one.

### User types

```elm
type Status = Up | Down
type Tree a = Leaf | Node (Tree a) a (Tree a)
```

Constructors are values; `Node : Tree a -> a -> Tree a -> Tree a`. An applied
constructor used as a field is parenthesized. Types may be recursive.

## Modules

One module per file. A module name maps to a path, a dot being a directory
separator: `import Fleet.Roles` resolves `Fleet/Roles.emet`.

```elm
module Fleet.Roles exposing (Role(..), packagesFor)

type Role = Web | Db

packagesFor : Role -> List Glyph
packagesFor role =
  case role of
    Web -> [ aptPackage { name = "nginx" } ]
    Db  -> [ aptPackage { name = "postgresql" } ]
```

```elm fragment
import Fleet.Roles as Roles exposing (Role(..))

host : String -> Role -> Scroll
host name role = scroll { name = name, glyphs = Roles.packagesFor role }
```

The header is optional; a file without one exposes everything and is a valid
entry module. `exposing (..)` exposes everything; an explicit list names values
and types, and `Type(..)` exposes a type's constructors along with it. An
`exposing` list may name only what the module itself declares — a module never
relays another's surface.

`import M` gives qualified access (`M.name`), `as` renames the qualifier, and
`exposing` brings the named values in unqualified. The case of the segment after
the last dot picks the namespace: `Fleet.Roles.packagesFor` is a value,
`Fleet.Roles.Role` a type or constructor.

A type and a constructor each belong to the module that declares it, so two
modules may both declare a `Thing` and be imported together. A bare name
resolves when exactly one candidate is in scope; with two, write the qualified
form at the use site.

Imports are acyclic; a cycle is an error.

**Search path.** `import Foo` resolves `Foo.emet` against the entry file's own
directory first, then each `source-directories` entry of the nearest `emet.json`
— found by walking up from the entry directory. First match wins. Without an
`emet.json`, resolution is entry-directory-only.

```json
{ "source-directories": ["lib"] }
```

## Prelude

Every module starts with the prelude in scope; nothing imports it. Names
carrying a `List.` / `Maybe.` / `String.` / `Char.` / `Tuple.` / `Secretspec.`
prefix are dotted identifiers resolved by lookup — the prefix is a naming
convention, and the module system above is separate from it.

The names and their signatures are listed once, in the published reference:
[`sites/website/src/content/docs/reference/language/prelude.mdx`](../../sites/website/src/content/docs/reference/language/prelude.mdx),
served at `/reference/language/prelude/`. It is the only copy; the registry it
transcribes is `apps/emet/src/prelude.rs`.

## Operators

Every operator desugars to a prelude function.

| Prec | Operators | Assoc | Meaning |
|---|---|---|---|
| 7 | `^` | right | power |
| 7 | `*` `/` `//` | left | multiply, float-divide, integer-divide |
| 6 | `+` `-` | left | add, subtract |
| 5 | `++` | right | append (`String` or `List`) |
| 5 | `::` | right | cons — prepend an element to a list |
| 4 | `==` `/=` `<` `>` `<=` `>=` | non-assoc | equality, comparison |
| 3 | `&&` | right | and |
| 2 | `\|\|` | right | or |

`/` is float division, `//` integer. Level 4 is non-associative, so `a < b < c`
is a parse error. Division and `modBy` / `remainderBy` by zero return `0` rather
than trapping. Unary `-x` desugars to `negate x`.

## Bounded type variables

Three closed constraints, as in Elm; no user typeclasses.

- `number` — `Int` or `Float`. An integer literal defaults to `Int` if nothing
  forces `Float`.
- `comparable` — `Int`, `Float`, `String`, `Char`, and tuples whose elements are
  all comparable, compared lexicographically. Unit is comparable and equal to
  itself.
- `appendable` — `String` or `List a`. This is what `++` requires.

`appendable` shares no admissible type with the other two, so it never merges
with them; `number` is inside `comparable`.

## Syntax

**Declarations.** `name args = body`, separated by layout. An optional signature
sits on the line above and is checked. Declarations are order-independent and
may recurse, including mutually: siblings are grouped into dependency cliques
before inference and evaluation, so one declaration may call another declared
below it.

```elm
isEven : Int -> Bool
isEven n = if n == 0 then True else isOdd (n - 1)

isOdd : Int -> Bool
isOdd n = if n == 0 then False else isEven (n - 1)
```

**Signatures and generics.** Hindley-Milner with let-generalization:
`map : (a -> b) -> List a -> List b`.

**Parameters.** A parameter is a name, or a parenthesized constructor
application that destructures the value where it binds.

```elm
type Config = Config { domain : String, port : Int }

domainOf : Config -> String
domainOf (Config spec) = spec.domain
```

The constructor must be the only one of its type — a parameter has no sibling
arms to cover what it misses. Every other pattern form is a parse error in
argument position; take the whole value and branch on it with `case … of`.

**Records.** `{ a = e }`, field access `e.a`. Inference is row-polymorphic, so
`\h -> h.name` and helpers that read fields off a record parameter type-check;
the record only needs the fields it uses. `{ r | a = e }` copies `r` with the
named fields replaced — type-preserving, so an update changes what a record
holds and never its shape. At least one field is required.

```elm
renamed : { name : String, port : Int } -> { name : String, port : Int }
renamed h = { h | name = "renamed", port = h.port + 1 }
```

**`let … in`.** Local bindings, layout-driven; single-line `let x = e in e`
works. Bindings in one `let` are grouped the same way top-level declarations
are, so they may reference each other in any order.

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

| Pattern | Matches |
|---|---|
| `n` | anything; binds it |
| `_` | anything; binds nothing |
| `0`, `-1` | an equal integer. Typed `number`, so it matches a `Float` scrutinee too. A float literal pattern is rejected. |
| `'a'` | an equal `Char` |
| `"nginx"` | an equal `String` |
| `Just x`, `Web` | that constructor, applied to sub-patterns |
| `[]` | the empty list |
| `(x :: xs)` | a non-empty list, binding head and tail |
| `[a, b]` | a list of exactly that length |
| `(a, b)`, `(a, b, c)` | a tuple, element-wise |
| `()` | unit |

A list is checked as a two-constructor sum (`[]` and `::`), so a `case` on one
covers both. A tuple has a single shape, so a tuple `case` is exhaustive exactly
when its element patterns are.

A built glyph matches by its PascalCase tag, and a filesystem glyph's `entry`
matches the same way:

```elm
describeGlyph : Glyph -> String
describeGlyph g =
  case g of
    AptPackage p     -> "apt ${p.name}"
    SystemdService s -> "unit ${s.unit}"
    LineInFile l     -> "line in ${l.path}"
    Filesystem f     ->
      case f.entry of
        File e      -> "file ${f.path} (${String.fromInt e.perms.mode})"
        Directory _ -> "dir ${f.path}"
        Symlink s   -> "link ${f.path} -> ${s.target}"
```

The tags are match-only: `aptPackage` builds, `AptPackage` matches. In the
projection a match sees, `mode` is the `Int` the compiler parsed, beside
`owner : Maybe String` and `group : Maybe String`.

**`if … then … else`.** Branches on a `Bool`; the same coverage as a two-arm
`True` / `False` `case`.

**Lambdas.** `\x y -> e`. Application is juxtaposition: `f x`. A lambda
parameter may destructure under the same rule as a declaration parameter:
`\(Config spec) -> spec.domain`.

**String interpolation.** `"port ${expr}"`, where `expr` is `String`-typed.
Desugars to `String.concat` and runs at compile time; combine with
`String.fromInt` to embed a number. The escapes inside `"…"` are `\n`, `\t`,
`\"`, `\\`, `\${`, and `\u{…}`.

**Char literals.** `'c'`, one Unicode scalar. The escapes are `\n`, `\t`, `\\`,
`\'`, and `\u{…}` with one to six hex digits.

**Tuples and unit.** `(a, b)` and `(a, b, c)` group values positionally; `()` is
unit. `(e)` is grouping — there is no one-tuple.

**Comments.** `--` to end of line.

## Secrets

`Secretspec.get : String -> String` reads a secret declared in
`secretspec.toml`. It is the one prelude name that reaches outside the program:
`emetc` fetches the value from the configured provider at compile time and
encrypts it to the fleet key, so the manifest carries a sealed hole rather than
the plaintext. `golemd` opens the hole as it writes the glyph.

```elm fragment
password : String
password = Secretspec.get "DB_PASSWORD"

main : List Scroll
main =
  [ scroll
      { name = "db"
      , glyphs =
          [ file
              { path = "/etc/app/db.env"
              , contents = "PASSWORD=${password}\n"
              , mode = "0600"
              }
          ]
      }
  ]
```

Compiling a program that calls it needs provider access and the fleet key
(`--secret-key`, or `GOLEM_SECRET_KEY_FILE`); a program that never calls it
needs neither. An undeclared key is a compile error naming the declared ones.

A sealed value composes into larger text through `++`, `String.append`,
`String.concat`, `String.join`, and interpolation, and other `String` functions
transform it into more sealed text. A function that would read something out of
it instead — `String.length`, `String.toInt` — is rejected, as is reaching an
identifier field: a path, a package or unit name, a mode, a symlink target, a
scroll name, or a `notifies` entry.

Of the two value-bearing fields, only a `file`'s `contents` accepts one, and the
mode has to match: any mode granting group or other read is rejected, so `"0600"`
is the mode to write while the surface constructors leave `group` unset.
`lineInFile` refuses a secret outright — that glyph owns one line and not the
permissions of the file around it.
