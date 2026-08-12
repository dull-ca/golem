# Talk diagrams

Two decks, generated as Excalidraw files by a small Python program.

- **`golem`** — twenty-two slides: what problem golem solves, how it works, how to
  use it.
- **`orchestration`** — seventeen slides: cloud orchestration from first
  principles, standing on its own but landing where golem lands.

Slides are written as code so that a colour, a label or a whole figure can change
in one place and every slide that shares it follows.

## Build

```
python docs/presentation/build.py
```

Python 3 stdlib only. No network, no install, no dependency beyond the `python3`
already in `devenv.nix`. It writes `dist/`, overwriting what is there; `--out DIR`
sends the files somewhere else.

**`dist/` is not in the repository.** Nothing exists until you run the build —
that includes the files the `restore()` check reads, so run `build.py` first.

## Open

```
dist/
  golem/01-what-you-buy.excalidraw … 22-plan-against-host.excalidraw
  golem/golem-deck.excalidraw
  orchestration/01-a-process-on-a-host.excalidraw … 17-where-golem-sits.excalidraw
  orchestration/orchestration-deck.excalidraw
  icons.excalidraw
```

Each `<name>-deck.excalidraw` holds every slide of that deck as a named Excalidraw
**frame**, laid out three to a row: one canvas, presentable as-is. Frame names
carry that deck's own slide numbers. `icons.excalidraw` is the icon vocabulary on
one sheet, labelled.

Open any of them at [excalidraw.com](https://excalidraw.com) (File → Open) or in
any Excalidraw editor. Hand-edit freely — nudge a box, retype a label, redraw an
arrow. The next `build.py` overwrites `dist/`, so move a hand-edited file
elsewhere or fold the change back into the slide module.

## The type scale

Four sizes, in `excalidraw/type_scale.py`. They are floors, not targets.

| Role | Size |
| --- | --- |
| Slide title | 46 |
| Section heading, band title | 30 |
| Body, card text, cell label | 24 |
| Caption, tag, axis note | 18 |

Nothing goes below 18 anywhere; `test_scenes.py` asserts it over every generated
text element. **A slide that will not fit is a slide that gets split.** Never
shrink type to win space — the split is the intended move, and deck length is not
a constraint.

The words budget is 35 per slide, excluding the title. It counts every word of
every text element, and on that count no slide meets it: the lowest in either
deck is 36 and the median is around 50, because the count cannot tell a sentence
from a label and most of what is left on a slide after the type floor is labels —
matrix headers, layer names, quoted Rust signatures, an `Install | Remove |
Replace | Noop`. So every slide carries a named ceiling in
`WORD_BUDGET_CEILINGS` in `test_scenes.py`, with the reason it needs one.

The ceiling is a ratchet rather than a target: it sits just above what the slide
draws today, so prose cannot creep back in without someone raising a number on
purpose. What the budget actually bought is visible against the deck it replaced,
which ran 90 to 250 words a slide on the same count.

## The layout vocabulary

Ten forms in `excalidraw/layout.py`. Reach for one before drawing rectangles, and
reach for one that is not already on the neighbouring slide — repeating a form is
how a deck starts to read as one slide shown many times.

| Form | Builder | Reads as |
| --- | --- | --- |
| responsibility matrix | `matrix` | who owns each cell of a grid |
| layered stack | `decks/golem/lichess_stack.draw` | what sits on what |
| radial hub | `hub_and_satellites` | one thing that is really several |
| swimlane pipeline | `swimlane`, `pipeline` | stages, in order, with flow |
| icon-led cards | `icon_card_row` | a sequence you recognise by its marks |
| state machine | `state_machine` | states and the transitions between them |
| before / after | `split_compare` | two claims held apart by a divider |
| cluster map | `cluster_map` | workloads sitting on hosts |
| card rhythm | `card_rhythm` | groups of unequal weight, 3 then 2 then 1 |
| timeline | `timeline` | a spectrum with named positions |
| coverage bars | `coverage_bars` | how far something reached, row by row |

Two rations hold in the golem deck: **at most two matrix slides**, and **the
six-layer lichess figure appears exactly twice** — slide 04 introduces it, slide
06 recolours it. Both draw it at identical geometry, so flipping between them
changes colour and nothing else.

## The icon vocabulary

Nineteen marks in `excalidraw/icons.py`, drawn from rectangles, ellipses and
lines. No image files, no emoji, no external assets.

```
container   container image   registry   host   cluster
scheduler   pending workload  binding    health probe   drift
network link   service   DNS / SRV lookup   load balancer
volume   secret   replica set   drain   rollback
```

Each is a function `(scene, x, y, size, *, tone=…) -> Mark`. `size` is the mark's
height; its width is `size × <NAME>_ASPECT`, and a caller needs that constant
before drawing to centre a mark in a card. The returned `Mark` carries the
elements drawn and the box declared.

The marks compose, and that is what keeps them consistent: `cluster` is `host`
repeated inside a dashed enclosure, `binding` is `pending_workload` over three
`host` marks, `replica_set` is `container` repeated. There is one `container`, so
a container looks the same in both decks.

`binding` is the one to understand. Assignment is an act, not a noun: an unplaced
workload, the nodes that could have taken it, and the one arrow that settled it.
The rejected candidates stay on the mark, faint and dashed.

No icon draws text, so none can breach the type floor at any scale.
`build.py` writes them all to `dist/icons.excalidraw` — which is also how the
`restore()` oracle covers marks that no slide happens to use.

## Add a slide

A slide module exposes exactly three names:

```python
SLUG = "what-you-buy"      # the filename stem
TITLE = "What you buy"     # the deck frame name

def build() -> Scene: ...  # draw into a Scene and return it
```

Write `decks/<deck>/sNN_your_slide.py`, then append its module name to
`SLIDE_MODULE_NAMES` in `decks/<deck>/__init__.py`. That tuple is the running
order and the only place it is written down — the slide number, the
`NN-slug.excalidraw` filename and the `NN · Title` frame name all derive from
position. Reordering the tuple renumbers the talk.

Inside `build()`, reach for `excalidraw.layout` and `excalidraw.icons` first, and
drop to the `Scene` primitives — `rectangle`, `text`, `arrow` — only for what no
builder covers. Font sizes come from `excalidraw.type_scale`, never a raw number.
Colours come from `excalidraw.palette` by meaning (`YOURS`, `PLATFORM`, `GAP`,
`WORKLOAD`, `NODE`), never by hex.

Strings both decks must agree on — the five names of the orchestration jobs —
live in `decks/vocabulary.py`. Import them; never retype one.

Then rebuild and check.

## Verify

Two layers, one always available and one better.

```
python docs/presentation/test_scenes.py
```

Builds every scene into a temporary directory, twice, and asserts the invariants:
required keys present, ids unique, every `containerId` and `boundElements[].id`
resolving both ways, every `frameId` resolving to a frame emitted before its
children, arrows anchored at `[0,0]` with matching `width`/`height`, no non-finite
numbers, labels that fit their containers, everything inside the canvas margin, no
text under the type floor, every slide inside its word budget, every icon inside
the box it declares, and two independent builds that are byte-identical. Offline,
stdlib `unittest`, no arguments.

```
python docs/presentation/build.py
cd docs/presentation/tools && bun install && bun run check
```

Loads every file in `dist/` — both decks and the icon sheet — through the real
`@excalidraw/excalidraw` `restore()`, the same call an editor makes on open, and
fails if any element comes back rewritten, dropped or `isDeleted`, or if the
z-order it derives is not strictly increasing. This is the only true oracle for
the format: the Python test checks the output against a restatement of the schema,
this checks it against the implementation. It needs a one-off network install
(`@excalidraw/excalidraw` and `jsdom`), which is why it is optional and lives
outside the build.

## Determinism

No wall clock, no RNG. Element `id`, `seed` and `versionNonce` come from
`blake2s(scene key + counter)`; `updated` is a fixed constant. Two builds of the
same source produce byte-identical files, which is why the generated files are not
tracked: anyone can reproduce them exactly, so the repository does not carry them.

```
python docs/presentation/build.py --out /tmp/a
python docs/presentation/build.py --out /tmp/b
diff -r /tmp/a /tmp/b
```

## Sharp edges

Each of these cost time once already.

**`index` is omitted on purpose.** Excalidraw's `restore()` regenerates the
fractional index from array order. A hand-rolled one that is not strictly
increasing corrupts z-order rather than setting it. Array order *is* the z-order —
append back-to-front. `test_scenes.py` asserts the key's absence.

**An arrow's `points` are relative to its `x,y`,** with `points[0] == [0,0]`, and
`width`/`height` are the span of the points. For an arrow travelling up or left the
visual bbox reaches back from the anchor, so `x + width` is not its right edge and
`x, y` is not its top-left corner. Any containment or bounds check must derive a
linear element's bbox from its points — `linear_extent` in `test_scenes.py`.

**Text width is an estimate.** Nothing loads a font. `text.py` carries a
per-character advance table for the hand font and a flat `MONOSPACE_ADVANCE` for
the mono font, and measurement must be told which. Measuring mono text with hand
metrics under-measures it, Excalidraw re-wraps the code literal on load, and the
layout computed at build time is not the layout on screen. That bug was real; two
tests now pin it against the font's true advance rather than the generator's own.

**Excalidraw's real bound-text padding is 5px,** not the generator's
`CONTAINER_PADDING`. The generator's larger value is slack for the width estimate,
so do not read it as the editor's number — `test_scenes.py` checks mono labels
against the real 5px.

**A card and the icon on it must not share a fill.** Give an icon-bearing card a
white fill and the icon the saturated tone, or the mark disappears into the card.
