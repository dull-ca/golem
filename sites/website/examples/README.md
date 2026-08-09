# Docs examples

Every line of Emet the documentation shows is compiled — the programs in this
directory (ADR 0043), and the fenced blocks on the pages too (ADR 0054). So is
every link between the pages. Nothing a reader can type is checked only by a
human having read it.

Three workspace tests do the checking. They run under `cargo test -p emet`, and
therefore under `nix flake check`:

- `apps/emet/tests/docs_examples.rs` — every `.emet` and `.emet-ref` here, plus
  its recorded error, its golden, and its mirror.
- `apps/emet/tests/docs_fences.rs` — every ```` ```emet ```` and ```` ```elm ````
  block under `docs/guide/` and `sites/website/src/content/docs/`, compiled
  against the `lib/` golem ships.
- `apps/emet/tests/docs_links.rs` — every link, anchor, and backticked
  repository path in the prose trees.

## Where should this code go?

**A file here, behind a `<Snippet>`,** when the page shows a program a reader
should be able to run, when the page shows the program's output, or when two
pages show the same code. Only a file can be run, carry a golden, or be pointed
at as a whole.

**A fence on the page** when the code exists to make one point in one place. It
is compiled either way, so it cannot rot; what it cannot do is any of the three
things above.

Both are checked. The choice is about what the reader gets, not about which one
is safe.

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

## Mirroring one, when a page needs it rearranged

A `.emet-ref` shows the real program as written. When a page needs it
rearranged instead — declarations moved so a `#region` can skip the ones the
lesson is not about, the module header and the design comments gone — write
that rendition here as an ordinary `.emet` and name what it stands for in a
`.mirrors` sidecar:

```
website-loop.emet
website-loop.mirrors     ->  examples/website/website.emet
```

Both are compiled and their `--text` plans are compared. Formatting, comments,
and declaration order are yours to change; the plan is not. A rendition that
quietly stops describing the real program fails, printing the lines where the
two plans diverge.

Reach for this only when a page genuinely needs the rearrangement. It is two
files where a `.emet-ref` is one, and the second one is a maintenance cost the
sidecar contains rather than removes.

## Fences on a page

A ```` ```emet ```` or ```` ```elm ```` block under `docs/guide/` or
`sites/website/src/content/docs/` is compiled against the real `lib/` — the same
library `emetc` resolves — so a page writing a `Quadlet.Workload` literal is
held to the record golem actually ships. (The two spellings are one language:
the site has its own Emet grammar, and `docs/guide/` is read through GitHub,
which highlights `elm` and has never heard of `emet`.)

A fence need not declare `main`; one is supplied when it does not, so a page can
teach a signature or a helper without the boilerplate around it.

### The `fragment` marker

A fence that is not a program on its own says so:

````
```emet fragment
{ path = "/etc/motd", contents = "…", mode = "0644" }
```
````

and the checker skips it. Because that is the only way out of the gate, it is
also the only way to silence the gate, so a second test compiles the marked
fences and **fails the ones that succeed**. The marker has to be earned: claim
it for something that does compile and the build tells you to drop it.

Reach for it for one expression, one signature, one field of a record — the
things that have no program around them. Where a fence is a few lines short of
compiling, adding those lines is usually the better page anyway: it is what a
reader would have to write.

One fence is marked for a different reason. The `Secretspec.get` program in
`docs/guide/reference.md` is complete and correct, but compiling it needs
provider access and a fleet key, neither of which a test has. The marker is
mechanically true there and semantically loose, and it is the only such case; if
a second complete secret-bearing example appears, the fences deserve a marker
that says *this one needs secrets* rather than borrowing the one that means
*this is not a program*.

## Links between pages

`docs_links.rs` reads the same prose, outside fences, and checks that:

- a relative link resolves to a file that exists;
- an `#anchor` names a heading that exists on the page it points at, slugged the
  way Astro slugs it — so an anchor that passes here is one a reader lands on;
- a site-absolute `/route/`, written on a site page, is a page the site
  publishes;
- a backticked `path/like/this.rs` whose first segment is a top-level directory
  of this repository is a file this repository has;
- no link points at `codeberg.org`, which this project moved off (ADR 0035).

Two exemptions, both deliberate. `docs/adr/` and `docs/design/` are exempt from
the repository-path check: they describe the tree as it stood on the day of the
decision, and correcting a path there would edit an accepted record. Everything
under `docs/superpowers/` is exempt entirely — dated implementation plans,
finished and left as written.

## What the tests enforce

- Every `.emet` and `.emet-ref` under this tree compiles — unless it has an
  `.expected-error`, in which case it must fail and its diagnostics must
  contain that text.
- Every `.text.golden` matches what the compiler renders right now.
- Every `.mirrors` sidecar names a program that plans what its rendition plans.
- Every documented fence compiles, unless marked `fragment` — and every
  `fragment` marker is one the fence needs.
- Every link, anchor, published route, and mentioned repository path resolves.
- Both trees are non-empty. If the flake's source filter ever stops copying them
  into the sandbox, the tests fail rather than silently checking nothing.

A failure names the file and the line and prints the diagnostic, the mismatched
lines, or the target that does not exist, so a CI log says what broke and how.
