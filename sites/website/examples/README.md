# Docs examples

Every Emet program the docs site publishes lives here as a real file that CI
compiles (ADR 0043). Pages never carry a copy of the code — they reference a
region of a file in this directory, so a page cannot drift from the program it
claims to show.

The checker is `apps/emet/tests/docs_examples.rs`, an ordinary workspace test.
It runs under `cargo test --workspace` and therefore under `nix flake check`.

## Adding an example

Write a compilable `.emet` program here and mark the slice the page shows:

```emet
-- #region currying
plus : Int -> Int -> Int
plus x y = x + y
-- #endregion

main : List Scroll
main = [ scroll { name = "example", glyphs = [] } ]
```

The compiler ignores `--` comments, so the markers cost nothing and the file
stays a program you can run:

```
cargo run -q -p emet -- build sites/website/examples/functions-currying.emet --text
```

A program still needs a `main`, so keep the boilerplate the page does not want
outside the region. Regions may repeat in one file — `maintenance-page.emet`
carries `template`, `include-line`, and `usage`.

Include it from a page:

```mdx
import Snippet from "../../../components/Snippet.astro";

<Snippet file="functions-currying.emet" region="currying" />
```

Paths are relative to this directory. A missing file, or a region name that is
not in it, fails the site build.

## Examples that must fail

A program that teaches a footgun has to keep failing, and keep failing *the
same way*. Put the diagnostic in a sidecar named after the example:

```
functions-shadowed-builtin.emet
functions-shadowed-builtin.expected-error
```

The sidecar holds a **substring** of the rendered diagnostic — one line of
`<phase> error: <message>` is the usual choice:

```
analysis error: evaluation exceeded recursion limit (possible infinite recursion)
```

The test asserts the compiler still fails and that its diagnostics contain that
text. If the language ever fixes the footgun, the example compiles and the test
fails — which is the point: the lesson goes stale loudly, not silently. Pages
show the recorded error with `<Output file="…​.expected-error" />`.

## Output blocks

A page that shows `emetc … --text` output includes a golden instead of a
transcription:

```mdx
import Output from "../../../components/Output.astro";

<Output file="hello-agent.text.golden" />
```

The golden is produced by the same renderer `emetc --text` calls, so it is the
tool's own output. To create one, make the empty file and generate it:

```
touch sites/website/examples/hello-agent.text.golden
UPDATE_DOCS_GOLDEN=1 cargo test -p emet --test docs_examples
```

That same command regenerates every golden after a deliberate change. Without
the variable the test asserts instead, printing the file name and the
mismatched lines.

`<Output file="…" scroll="manta" />` prints one root scroll's stanza (plus the
two header lines) out of a multi-host golden — useful when the whole plan runs
to hundreds of lines. The slice is cut from the golden mechanically, so it
cannot drift either; a scroll that disappears fails the site build.

## Referencing a real fleet program

The real programs under the repo-root `examples/` stay where they are. To show
one's output, drop a `.emet-ref` here whose contents are the repo-relative path
to it, and name the golden after the ref:

```
litour.emet-ref          ->  examples/lichess/fleet.emet
litour.text.golden
```

The test compiles the referenced program and asserts the golden exactly as it
does for a local example. No copy of the fleet lives here.

## What the test enforces

- Every `.emet` and `.emet-ref` under this tree compiles — unless it has an
  `.expected-error`, in which case it must fail and its diagnostics must
  contain that text.
- Every `.text.golden` matches what the compiler renders right now.
- The tree is non-empty. If the flake's source filter ever stops copying this
  directory into the sandbox, the test fails rather than silently checking
  nothing.

A failure names the file and prints the diagnostic or the mismatched lines, so
a CI log says which example broke and how.
