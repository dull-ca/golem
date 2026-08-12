# Talk diagrams

Thirteen slides for the golem talk — what problem it solves, how it works, how to
use it — generated as Excalidraw files by a small Python program. Slides are
written as code so that a colour, a label or a whole layer can change in one place
and every slide that shares the figure follows.

## Build

```
python docs/presentation/build.py
```

Python 3 stdlib only. No network, no install, no dependency beyond the `python3`
already in `devenv.nix`. It writes `dist/`, overwriting what is there; `--out DIR`
sends the files somewhere else.

## Open

`dist/` holds one file per slide plus a combined deck:

- `dist/01-what-you-buy.excalidraw` … `dist/13-plan-before-apply.excalidraw`
- `dist/deck.excalidraw` — every slide as a named Excalidraw **frame**, laid out
  three to a row. One canvas, presentable as-is.

Open any of them at [excalidraw.com](https://excalidraw.com) (File → Open) or in
any Excalidraw editor. Hand-edit freely — nudge a box, retype a label, redraw an
arrow. The next `build.py` overwrites `dist/`, so move a hand-edited file
elsewhere or fold the change back into the slide module.

## The thirteen slides

| # | Title | Module |
|---|---|---|
| 01 | What you buy | `slides/s01_what_you_buy.py` |
| 02 | What you configure | `slides/s02_what_you_configure.py` |
| 03 | What lichess runs | `slides/s03_lichess_stack.py` |
| 04 | What orchestration means | `slides/s04_orchestration.py` |
| 05 | If we bought orchestration | `slides/s05_bought_orchestration.py` |
| 06 | Where we were: Ansible | `slides/s06_ansible.py` |
| 07 | December: containers | `slides/s07_december_containers.py` |
| 08 | December: the plumbing | `slides/s08_december_plumbing.py` |
| 09 | Where it broke | `slides/s09_where_it_broke.py` |
| 10 | What golem is, and is not | `slides/s10_what_golem_is.py` |
| 11 | The pipeline | `slides/s11_pipeline.py` |
| 12 | Emet and the four glyphs | `slides/s12_emet_glyphs.py` |
| 13 | Plan before apply | `slides/s13_golemctl_golemd.py` |

Slides 03, 05, 06 and 07 are the same six-layer figure four times. It lives in
`slides/lichess_stack.py` and is recoloured by each caller, never redrawn: change
a layer's wording there and all four slides change together.

## Add a slide

A slide module exposes exactly three names:

```python
SLUG = "what-you-buy"      # the filename stem
TITLE = "What you buy"     # the deck frame name

def build() -> Scene: ...  # draw into a Scene and return it
```

Write `slides/s14_your_slide.py`, then append its module name to
`SLIDE_MODULE_NAMES` in `slides/__init__.py`. That tuple is the running order and
the only place it is written down — the slide number, the `NN-slug.excalidraw`
filename and the `NN · Title` frame name are all derived from position. Reordering
the tuple renumbers the talk.

Inside `build()`, reach for `excalidraw.layout` first: `slide_header`, `matrix`,
`panel`, `pipeline`, `box_row`, `text_card`, `legend`, `callout`, `note`. Drop to
the `Scene` primitives — `rectangle`, `text`, `arrow` — only for the parts a
builder does not cover. Colours come from `excalidraw.palette` by meaning
(`YOURS`, `PLATFORM`, `ANSIBLE`, `GAP`), not by hex.

Then rebuild and commit `dist/`.

## Verify

Two layers, one always available and one better.

```
python docs/presentation/test_scenes.py
```

Builds every scene into a temporary directory and asserts the invariants: required
keys present, ids unique, every `containerId` and `boundElements[].id` resolving
both ways, every `frameId` resolving to a frame emitted before its children,
arrows anchored at `[0,0]` with matching `width`/`height`, no non-finite numbers,
labels that fit their containers, everything inside the canvas margin, and a
repeat build that is byte-identical. Offline, stdlib `unittest`, no arguments.

```
cd docs/presentation/tools && bun install && bun run check
```

Loads every file in `dist/` through the real `@excalidraw/excalidraw` `restore()`
— the same call an editor makes on open — and fails if any element comes back
rewritten, dropped or `isDeleted`, or if the z-order it derives is not strictly
increasing. This is the only true oracle for the format: the Python test checks
the output against a restatement of the schema, this checks it against the
implementation. It needs a one-off network install (`@excalidraw/excalidraw` and
`jsdom`), which is why it is optional and lives outside the build.

## Determinism

No wall clock, no RNG. Element `id`, `seed` and `versionNonce` come from
`blake2s(scene key + counter)`; `updated` is a fixed constant. Two builds of the
same source produce byte-identical files.

That is what makes the committed `dist/` worth having: a diff there is a real
change to a diagram, not timestamp noise, and it can be reviewed. **Rebuild and
commit `dist/` in the same change as the slide it came from**, or the committed
files stop describing the code.

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
