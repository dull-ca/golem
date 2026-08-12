# Presentation — what we decided, and why

A design log for the two decks under `docs/presentation/`. `SPEC.md` records what
each slide says; this records the decisions behind them, including the ones we got
wrong first and had to reverse. It exists so a later session — or a later
conversation with Dr. Dub — starts from the conclusions rather than re-deriving them.

Four rounds of review produced everything below. Where a decision replaced an earlier
one, both are here; the reversals are the most useful part of the file.

## What exists

Two decks, generated from Python, never hand-authored as JSON.

- **`decks/golem/`** — 33 slides. The talk: what problem golem solves, how it works,
  how to use it.
- **`decks/orchestration/`** — 17 slides. A standalone primer on cloud orchestration
  that stands on its own and lands on where golem sits.
- **`excalidraw/`** — the generator. `scene.py` element factories, `layout.py`
  composite forms, `icons.py` a drawn mark vocabulary, `type_scale.py` the font
  floors, `palette.py` semantic tones, `text.py` font-advance estimation, `assets.py`
  offline image embedding.
- **`build.py`** — emits per-slide files plus a combined `*-deck.excalidraw` per deck,
  each slide a named Excalidraw frame.

## The decisions

### Generate the diagrams; never hand-edit the JSON

The whole point. A slide is a Python module exposing `build() -> Scene`. Shared
figures are authored once and reused, which is what makes a fleet-wide restyle a
one-line change rather than thirty-nine edits.

### The build is deterministic, and `dist/` is not committed

No wall clock, no RNG — element ids and seeds are hashed from a scene key plus a
counter. `dist/` was tracked at first and was **46,914 of the 51,311 lines** on the
branch. Determinism was the argument for committing it and is actually the argument
against: anyone can regenerate the identical tree. Determinism is now checked by
diffing two independent `--out` builds, not against a tracked tree.

### Type has floors, and a slide that does not fit gets split

Title 46, section heading 30, body 24, caption 18. Nothing below 18. Roughly 35 words
of body per slide. **Split, never shrink** — this is why the golem deck went 13 → 22 →
29 → 33 slides. Tests assert the floor and the budget, so violating it fails the
build rather than being a matter of taste.

### Prose is explanation, in plain language, defining things by what they are

The first draft built every slide to one template: a portentous subtitle plus an
epigram closer, 78 of them. "What you stop operating, you start depending on."
"Nothing is ever 'done' — the loop is the product." Dr. Dub's verdict: "typical Claude
terminology", "poppycock".

The fix was to **delete the slots, not rewrite the words** — 24 of 39 subtitles and 14
closing lines went outright. The rules that replaced them:

- Every slide is one Diataxis mode, recorded in `SPEC.md`. These decks are
  **explanation**, with a handful of genuine **reference** slides. Reference slides
  state what exists, flatly; a verb list is a verb list.
- The first appearance of a term gets one flat declarative sentence saying what it is.
  "An image is read-only filesystem layers plus a config, named by the digest of its
  contents" — not "Layered, content-addressed, and never edited in place."
- **The test that kills the rest:** could a competent engineer disagree with this
  sentence on technical grounds? If not, it is rhetoric. Cut it.
- Banned: aphorisms, rhetorical antithesis, fragments used for weight, rule-of-three
  cadence, tone words, and anthropomorphised tools — a playbook does not "reach", an
  image does not "carry".
- **Labels are read cold.** A short label must be a noun phrase naming what the box
  is, never a clause depending on a sentence elsewhere on the canvas. This rule exists
  because a hub once shipped captioned "Layer 6 / one word" — a fragment of the
  slide's own subtitle, snapped off and left floating.

The speaker says the sentences. The slide holds the nouns.

### The six layers are the deck's colour vocabulary

Slide 05's numbering and tones are the spine: 1 core OS/network/security (slate),
2 application hosting (teal), 3 connective infrastructure (blue), 4 tools,
dependencies, runtimes (violet), 5 the applications (green), 6 lifecycle/schedule/
scaling (orange). Everything downstream imports those constants rather than matching
by eye.

### Layers are categories of work, not strata inside a machine

The correction that made the fleet sequence work. The first version drew each machine
as a miniature of slide 05, with the six layers stacked inside the box. That asserts a
structure which does not exist. **A layer is an activity a tool performs on a
machine.** Layer 1 is the work of configuring the core OS, network and security;
layer 5 is the work of running the applications.

What replaced it, in the fleet frames:

- **A machine box holds its units** — the containers and applications actually on that
  host.
- **The layer colours ride on the tools and their actions** — the chips, the
  connectors, the legend.
- **A machine's border** says which tool did machine-level work there; **the cells
  inside** say who keeps each unit.

Two independent channels, which is what lets a single frame say *"Ansible touches all
thirty machines and keeps units on eight"*. Coverage across the fleet and depth of
work stopped being conflated.

### Hand-managed versus tool-managed must be shown honestly

Lichess has a system that aims to manage the fleet and it does not manage all of it.
An earlier frame showed all machines under golem, which overstated where lichess is.
Every fleet frame now shows the mix. The closing golem frame says **8 machines
covered, 22 still by hand**.

### The fleet is drawn from the real inventory

Machine names and the managed/unmanaged split come from
`lichess-sysadmin/ansible/inventory/hosts.yaml`, not from invented plausible data.
`managed` means *whether Ansible manages this* and **defaults to `True`**, so every
explicit `managed: false` is a unit that exists but Ansible does not keep.

Derived figures, **worth sanity-checking**: 30 hosts, 82 units, 22 tool-kept, 60
`managed: false`. Hosts with tool-kept units: `achoo` 2, `apate` 1, `cobar` 5,
`dingo` 3, `lucid` 1, `orbit` 8, `radio` 1, `thonk` 1. Ten hosts marked `?`.

Two traps in parsing that file: ingress entries key on `- domains:` while services and
workloads key on `- name:`, and **host vars are split across groups** — the `mongodb:`
group carries `databases:` blocks that `all:` does not. Reading `all:` alone gives 42
unmanaged; the merge gives 60, matching `grep -c "managed: false"`.

Portainer's host is not in the inventory at all, so the slide gives the count and
names no host.

### The empty state recedes; the filled state is what the eye lands on

Unconfigured is dotted and very light, not dashed and mid-grey. The contrast is solved
from the empty side rather than by shouting the configured colours louder. An earlier
version filled layer 1 in a slate so pale that the first beat of the sequence — the
whole fleet gaining a baseline — was invisible.

### Animation is not available, so build-up is carried by frames

We designed one round around Excalidraw+ interpolating elements that persist between
frames. **That was unverified and is probably wrong.** Excalidraw's own presentations
page documents no transition behaviour; the interpolation claims are third-party and
never state the matching key. Element ids must be unique within a canvas, so ids
cannot be shared across frames on a merged deck at all — the merge step renames
collisions, which is a plausible reason nothing animates.

So every progression is a static frame. The golem pipeline became four frames —
compiled, dispatched, assembling, converged — rather than one frame captioned "over
time". A probe file exists in the session scratchpad if anyone wants to settle the
question; nothing depends on the answer.

### Icons are drawn from primitives; the golem emblem is the one import

Twenty marks in `icons.py`, no external assets. The exception is golem's own emblem —
**robot-golem by Lorc, game-icons.net, CC BY 3.0**, vendored under `assets/`, embedded
as a data URL so the build stays offline, credited in four places.

It is used **once and large**, as an identity mark. The per-machine golems are drawn
diamonds instead: a dense filled silhouette among nineteen open line marks reads as a
different medium, and blobs below about 40px.

## Corrections that changed the argument, not the wording

Three framing errors were caught in review. They matter more than any styling
decision, because each one made the deck claim something untrue.

### Manual placement is a choice, not a defect

The deck indicted human placement — "A human chose the host. Every time.",
"Placement lived in a Python file and in a human's head." **golem is deliberately not
a scheduler.** A person still chooses the host, and that is wanted. The orchestration
deck had this right while the golem deck argued the opposite; the golem deck was the
one that changed.

### What December lacked was mechanism, not judgement

Moving a service from host A to host B meant: edit the definition to disable it, apply
to A, edit again to add it to B and remove it from A, apply. Edits and applies ordered
by hand; wrong order and it runs on both hosts or neither. **The defect is that "move A
to B" was not expressible as one change** — not that a person decided it.

### golem improves expressibility, not choreography

golem does **not** sequence a cross-host move. Changing which host a service belongs to
installs it on B and removes it from A, both falling out of the same manifest diffed
per host. So the honest contrast is *edits and applies by hand* versus *one edit, one
apply*. Nothing orders the two hosts, so a window exists where the service runs on both
or neither. The slide says so.

## Claims corrected against the code

Reviewing prose surfaced seven claims that were wrong, five drifted and two invented:

- `reconcile::plan` takes `&[Outcome]`, not two scrolls
- `Reconciler::apply` and `reverse` are fallible
- `/plan` and `/reconciles/:id` are bare paths
- `observe` is a trait method
- probes are `dpkg-query`, `systemctl` and direct filesystem reads — nothing scoped to
  `/etc`
- **golem has no drain.** No drain operation exists in `apps/` or `libs/`, and none is
  proposed in any ADR or TODO. The deck had claimed one.
- **"no agent" contradicted golemd.** golemd is an agent.

`RevisionKind` carries `Init`/`Reconcile`, not `Revision`; `Inverse` is a field of
`Outcome`, and `reverse` takes the whole `Outcome`.

## The verification bar

Every round must pass, and each is run for real rather than asserted:

1. `python docs/presentation/build.py` — clean, stdlib only, offline
2. two `--out` builds, `diff -r` clean
3. `python docs/presentation/test_scenes.py` — includes the type floor, the word
   budget, geometry invariants, and the icon bounding boxes
4. `cd docs/presentation/tools && bun install && bun run check` — every generated file
   through Excalidraw's real `restore()` under jsdom, confirming no element is
   rewritten, dropped, or reordered. This is the only true oracle for the format; the
   Python test is the always-offline check.
5. nothing outside `docs/presentation/`

## Open questions

- **What language Emet actually resembles.** The original description, "inspired by
  (nearly identical to) emet", is self-referential. Nothing false is drawn — the slide
  says only that a typed, functional program evaluates to one Scroll per host — but the
  comparison needs an answer before it is made out loud.
- **Whether the inventory-derived numbers above are right.** They come from a
  hand-written indent parser, since `yaml` is not in the golem devenv.
- **Four things verified only by reasoning:** Excalidraw's real dotted stroke (the
  check renderer approximates it with a dash array), roughness (the editor draws
  sketchier strokes than the check renders), the embedded emblem (never rasterised),
  and projector legibility (judged from 1600px renders, not a projector).
- **Whether "workloads, quadlets and ingress are Emet libraries that compile down to
  the four glyphs"** holds. It is asserted in `CLAUDE.md` and was not confirmed against
  Emet source.
