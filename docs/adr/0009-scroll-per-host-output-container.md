# 0009-scroll-per-host-output-container

## Status

Accepted.

## Context

Today an Emet program evaluates to a flat `List Glyph` — an IR list of resource
primitives (`AptPackage`, `SystemdService`, and soon `File`/`LineInFile`, per
ADR 0002). A real fleet tool must group those glyphs by the target machine they
apply to: the same glyph key (e.g. install `nginx`) is entirely legitimate on
two different hosts, but is a genuine conflict on one. The flat bottom has no
level at which to express "these glyphs belong to *this* machine", so a
machine-level container is needed.

Hosts sit a level *above* resources. This container depends on `List` being
first-class (design §5 / Wave 2), which is landing now, so `List Scroll` and
`List Glyph` are both expressible.

## Decision

Introduce **`Scroll`** — an opaque, nominal, per-host output container — and make
it the program's output bottom.

- **IR node (`src/ir.rs`).** Add an opaque IR node
  `Scroll { name: String, glyphs: Vec<Glyph> }` — a sibling to `Glyph` but one
  level *up*: a `Scroll` *contains* glyphs. Start with just `name` + `glyphs`;
  richer machine attributes (IPv4 / IPv6 / hostname / …) are deferred and can be
  added later without disturbing this shape.
- **Surface constructor.** A reserved lowercase **record constructor**
  `scroll { name = String, glyphs = List Glyph } : Scroll`, in the same family as
  the glyph constructors (`aptPackage` / `systemdService`, ADR 0002). It is a
  **nominal opaque node, NOT a general `{…}` record** — the IR boundary stays
  clean.
- **`Scroll` is its own type** `Con("Scroll", [])`. It is **not** a glyph and does
  **not** inject into `Glyph` (contrast the glyph subsumption of ADR 0002).
- **The program's output bottom becomes `main : List Scroll`** — the sole output
  shape. A single-host config is `[ scroll {…} ]`. `run_module` produces
  `Vec<Scroll>`; `lib.rs`'s `Compiled` and `main.rs`'s rendering group output per
  scroll.
- **`analyze` conflict-detection moves INSIDE each scroll.** Glyph key-conflicts
  are detected *per-scroll*, so two different scrolls (hosts) may share glyph keys
  (both install `nginx`) without a false conflict; conflicts are still caught
  within a single scroll.
- **`name` is a label for now.** No cross-scroll `name`-uniqueness is enforced
  yet — identity / keying by `name` (and the richer machine attributes) comes with
  the later machine-attributes work.

## Alternatives considered

1. **Keep the flat `List Glyph` bottom and attach host info onto glyphs.**
   Rejected: hosts are a level *above* resources; folding host identity into each
   glyph conflates the two levels and muddies both the IR and conflict detection
   (per-host grouping would have to be reconstructed from glyph fields).
2. **Model `Scroll` as a plain record / `type alias { name, glyphs }`.** Rejected:
   we want an opaque nominal IR node — a clean integration boundary for the larger
   program that consumes the IR — and `type alias` is an unimplemented non-goal
   (design §13, deferred).
3. **Name it `Host` (literal) or `Golem` (max-theme).** Chosen name is **`Scroll`**
   for thematic coherence with `Glyph`: in the golem legend the creature is
   animated by a *scroll* (shem) bearing its inscription, so a scroll is exactly
   "the parchment of glyphs for one machine." `Host` is the reasonable literal
   alternative; `Golem` was rejected because it collides with the `golem`
   ecosystem name.

## Consequences

- `main`'s required type changes from `List Glyph` to `List Scroll`. The demo and
  `examples/*.emet` move to `[ scroll { name = …, glyphs = [ … ] } ]`.
- `src/ir.rs` gains the `Scroll` node; output becomes `Vec<Scroll>`; `analyze`
  runs per-scroll.
- The IR handed to the "larger program" is now a `List Scroll` — the clean
  fleet-level boundary between the Emet language and its consumer.
- Cross-references: builds on the glyph primitives (ADR 0002) and needs
  first-class `List` (design §5 / Wave 2). Placed alongside the `file` /
  `lineInFile` primitives wave in the staged plan (design §13).
