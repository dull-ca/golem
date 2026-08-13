# Talk diagrams

Three decks, generated as Excalidraw files by a small Python program.

- **`golem`** — thirty-three slides: what problem golem solves, how it works, how
  to use it. Slides 14 to 23 are one sequence over the thirty machines the lichess
  Ansible inventory names, and what changes between frames is which units sit on
  them and who keeps each one.
- **`orchestration`** — twenty-six slides: cloud orchestration from first
  principles, standing on its own but landing where golem lands. Slides 08 to 10
  answer the five jobs three times over — today, December's plan, with golem —
  and it closes on a seven-band stack drawn three times: who sells each part,
  which product answers it, and which part is golem's.
- **`machine-lifecycle`** — ten slides: the five steps that bring one lichess
  machine into service, four of which are done by hand today, and one tool per
  span instead. Slides 01 and 05 are the same five-step band at the same
  geometry, differing only in who covers what.

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
  golem/01-what-you-buy.excalidraw … 33-plan-against-host.excalidraw
  golem/golem-deck.excalidraw
  orchestration/01-a-process-on-a-host.excalidraw … 26-where-golem-sits.excalidraw
  orchestration/orchestration-deck.excalidraw
  machine-lifecycle/01-today.excalidraw … 10-what-changes.excalidraw
  machine-lifecycle/machine-lifecycle-deck.excalidraw
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
| layered stack | `decks/golem/lichess_stack.draw`, `decks/orchestration/stack.draw` | what sits on what |
| radial hub | `hub_and_satellites` | one thing that is really several |
| swimlane pipeline | `swimlane`, `pipeline` | stages, in order, with flow |
| icon-led cards | `icon_card_row` | a sequence you recognise by its marks |
| state machine | `state_machine` | states and the transitions between them |
| before / after | `split_compare` | two claims held apart by a divider |
| cluster map | `cluster_map` | workloads sitting on hosts |
| card rhythm | `card_rhythm` | groups of unequal weight, 3 then 2 then 1 |
| timeline | `timeline` | a spectrum with named positions |
| coverage bars | `coverage_bars` | how far something reached, row by row |
| machine fleet | `decks/machines.draw_fleet`, `decks/golem/fleet.draw` | which units sit on which machine, and who keeps each one |
| answered list | `decks/orchestration/job_answers.draw` | a named list of jobs, and who answers each row |
| step band | `decks/machine_lifecycle/lifecycle.draw` | ordered steps, and who covers each stretch of them |
| ordered play | `decks/ansible_play.draw_play` | numbered steps run top to bottom |

Two rations hold in the golem deck: **at most two matrix slides**, and **the
six-layer lichess figure appears exactly twice** — slide 05 introduces it, slide
07 recolours it. Both draw it at identical geometry, so flipping between them
changes colour and nothing else.

Eight modules hold a figure that more than one slide draws:
`decks/golem/lichess_stack.py` (slides 05 and 07), `decks/golem/lichess_ladder.py`
(03 and 04), `decks/golem/fleet.py` (14 to 23), `decks/machines.py` (those frames,
the orchestration deck's 22 and 24, and the lifecycle deck's 03, 04 and 09),
`decks/orchestration/stack.py` (20, 21 and 26),
`decks/orchestration/job_answers.py` (08, 09 and 10), `decks/ansible_play.py` (the
orchestration deck's 23 and the lifecycle deck's 03) and
`decks/machine_lifecycle/lifecycle.py` (01 and 05). Each takes state and no geometry — the constants inside are the figure,
and a slide that passed its own size would make the same figure jump between
slides.

Anything more than one deck draws lives in `decks/`, not under one of them. A
copy would drift, and the decks would stop reading as the same fleet.

## The icon vocabulary

Twenty-two marks in `excalidraw/icons.py`, drawn from rectangles, ellipses and
lines. No emoji, and one image file: golem's own symbol, in `assets/`.

```
container   container image   registry   host   cluster
scheduler   pending workload  binding    health probe   drift
network link   service   DNS / SRV lookup   load balancer
volume   secret   replica set   drain   rollback
source file   operating system install   disk layout
```

Each is a function `(scene, x, y, size, *, tone=…) -> Mark`. `size` is the mark's
height; its width is `size × <NAME>_ASPECT`, and a caller needs that constant
before drawing to centre a mark in a card. The returned `Mark` carries the
elements drawn and the box declared.

The marks compose, and that is what keeps them consistent: `cluster` is `host`
repeated inside a dashed enclosure, `binding` is `pending_workload` over three
`host` marks, `replica_set` is `container` repeated. There is one `container`, so
a container looks the same in every deck.

Two marks the fleet frames need live in `decks/machines.py` rather than here: the
machine box and the scroll. Both carry state that an icon does not — a host name,
a count of units, who keeps each one — so they take a `Machine` rather than a
size and a tone.

`binding` is the one to understand. Assignment is an act, not a noun: an unplaced
workload, the nodes that could have taken it, and the one arrow that settled it.
The rejected candidates stay on the mark, faint and dashed.

A mark takes one tone and uses it for everything it draws, so a card row can
recolour a mark without reaching past the first keyword argument. The composites
that carry a second tone — `drift`, `binding`, `drain` — default it, and
`os_install` defaults it to the tone it was given.

No icon draws text, so none can breach the type floor at any scale.
`build.py` writes them all to `dist/icons.excalidraw` — which is also how the
`restore()` oracle covers marks that no slide happens to use.

## The one imported mark

`assets/robot-golem.svg` — **by Lorc, from game-icons.net, under CC BY 3.0** —
is golem's symbol, embedded as an Excalidraw image element on slide 22.
Attribution is required by the licence and is carried in `assets/README.md`,
`SPEC.md`, here, and on the slide itself.

Committed, never fetched: `build.py` reads it from disk and base64-encodes it into
the document's `files` map, with `created` and `lastRetrieved` set to the
generator's fixed constant rather than a clock, so the build stays offline and
byte-identical.

Reach for it for golem's identity, not as a nineteenth icon. A dense filled
silhouette beside open line drawings reads as a different medium, and it turns to
a blob under about 40px — everywhere a mark has to be small or repeated, draw
one.

## Add a slide

A slide module exposes exactly three names:

```python
SLUG = "what-you-buy"      # the filename stem
TITLE = "What you buy"     # the deck frame name

def build() -> Scene: ...  # draw into a Scene and return it
```

Write `decks/<deck>/sNN_your_slide.py`, then append its module name to
`SLIDE_MODULE_NAMES` in `decks/<deck>/__init__.py`, whose package name is in
`DECK_PACKAGE_NAMES` in `decks/__init__.py`. That tuple is the running
order and the only place it is written down — the slide number, the
`NN-slug.excalidraw` filename and the `NN · Title` frame name all derive from
position. Reordering the tuple renumbers the talk.

Inside `build()`, reach for `excalidraw.layout` and `excalidraw.icons` first, and
drop to the `Scene` primitives — `rectangle`, `text`, `arrow` — only for what no
builder covers. Font sizes come from `excalidraw.type_scale`, never a raw number.
Colours come from `excalidraw.palette` by meaning (`YOURS`, `PLATFORM`, `GAP`,
`WORKLOAD`, `NODE`) or by tool (`ANSIBLE`, `PULUMI`, `GOLEM`), never by hex.

Strings both decks must agree on — the five names of the orchestration jobs —
live in `decks/vocabulary.py`. Import them; never retype one.

The fleet's hosts and unit counts live in `decks/lichess_fleet.py`, derived from
`lichess-sysadmin/ansible/inventory/hosts.yaml` and written out so a reviewer can
check them against it. Host names and counts only — the inventory is full of
addresses, MACs and key names, and none of that goes on a slide.

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
the box it declares, every embedded file referenced by an image element and
stamped with the fixed timestamp, and two independent builds that are
byte-identical. Offline,
stdlib `unittest`, no arguments.

```
python docs/presentation/build.py
cd docs/presentation/tools && bun install && bun run check
```

Loads every file in `dist/` — all three decks and the icon sheet — through the real
`@excalidraw/excalidraw` `restore()`, the same call an editor makes on open, and
fails if any element comes back rewritten, dropped or `isDeleted`, or if the
z-order it derives is not strictly increasing. This is the only true oracle for
the format: the Python test checks the output against a restatement of the schema,
this checks it against the implementation. It needs a one-off network install
(`@excalidraw/excalidraw` and `jsdom`), which is why it is optional and lives
outside the build.

## Determinism

No wall clock, no RNG. Element `id`, `seed` and `versionNonce` come from
`blake2s(id namespace + counter)`; `updated`, and an embedded file's `created` and
`lastRetrieved`, are a fixed constant. Two builds of the
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

**Scenes may share an id namespace; one canvas may not share an id.** The wide
fleet frames pass `id_namespace=` so an unchanged element keeps its id across
them, which is legal because they are separate documents. The combined deck merges
them onto one canvas, where a duplicate id makes `restore()` reissue one at random
and drop the bindings pointing at it — `framed_deck` renames collisions as it
merges. SPEC.md, "Stable ids across a sequence", has the reasoning.

**No frame may rely on a transition.** Excalidraw+'s Present mode is not known to
interpolate anything, and the shared id namespace is not a mechanism to lean on —
SPEC.md, "Excalidraw+ transitions are unverified". A build-up is spelled out as
extra frames, each of which reads on its own.

**Faint and dotted means *not yet*.** An unconfigured machine takes `INK_GHOST`
with `strokeStyle: "dotted"` — lighter than anything else on the canvas, so the
eye lands on what a tool has done. Use the same treatment for any not-yet state
rather than inventing a per-slide one; a second idiom for the same meaning is how
a viewer stops trusting either.
