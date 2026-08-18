# Presentation — what we decided, and why

A design log for the decks under `docs/presentation/`. `SPEC.md` records what
each slide says; this records the decisions behind them, including the ones we got
wrong first and had to reverse. It exists so a later session — or a later
conversation with Dr. Dub — starts from the conclusions rather than re-deriving them.

Everything below came out of a review round. Where a decision replaced an earlier
one, both are here; the reversals are the most useful part of the file.

## What exists

Three decks, generated from Python, never hand-authored as JSON.

- **`decks/golem/`** — 47 slides. A title slide, the talk — what problem golem
  solves, what was wanted before it was built, how it works, where it falls
  short, and what is asked of the room — and a nine-slide appendix. Strings two
  of its own slides must agree on live in modules beside the slides:
  `goals.py` (the five goals), `scorecard.py` (the seven graded rows) and
  `playbook.py` (the four play steps and the fleet state each leaves).
- **`decks/orchestration/`** — 26 slides. A standalone primer on cloud orchestration
  that stands on its own and lands on where golem sits.
- **`decks/machine_lifecycle/`** — 10 slides. How a lichess machine comes to exist,
  and which tool should take each of the five steps.
- **`decks/`** above them — `vocabulary.py` (the five job names), `lichess_fleet.py`
  (the thirty hosts), `machines.py` (the machine box and the fleet layout) and
  `ansible_play.py` (the play as an ordered list), all shared because more than one
  deck draws them.
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
one-line change rather than one edit per slide.

### The build is deterministic, and `dist/` is not committed

No wall clock, no RNG — element ids and seeds are hashed from a scene key plus a
counter. `dist/` was tracked at first and was **46,914 of the 51,311 lines** on the
branch. Determinism was the argument for committing it and is actually the argument
against: anyone can regenerate the identical tree. Determinism is now checked by
diffing two independent `--out` builds, not against a tracked tree.

### Type has floors, and a slide that does not fit gets split

Title 46, section heading 30, body 24, caption 18. Nothing below 18. Roughly 35 words
of body per slide. **Split, never shrink** — this is why the golem deck went 13 → 22 →
29 → 33 slides. The thirty-fourth is the title slide, which is not a split; the
thirteen after that are the playbook block, the goals, the caveats, the grading and
the ask, which took it to 47. Two of those thirteen exist because a slide would not
fit: the seven graded rows became slides 36 and 37. Tests assert the floor and the
budget, so violating it fails the build rather than being a matter of taste.

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

Slide 41's numbering and tones are the spine: 1 core OS/network/security (slate),
2 application hosting (teal), 3 connective infrastructure (blue), 4 tools,
dependencies, runtimes (violet), 5 the applications (green), 6 lifecycle/schedule/
scaling (orange). Everything downstream imports those constants rather than matching
by eye.

### Layers are categories of work, not strata inside a machine

The correction that made the fleet sequence work. The first version drew each machine
as a miniature of slide 41, with the six layers stacked inside the box. That asserts a
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

Twenty-seven marks in `icons.py`, no external assets. The exception is golem's own emblem —
**robot-golem by Lorc, game-icons.net, CC BY 3.0**, vendored under `assets/`, embedded
as a data URL so the build stays offline, credited in four places.

It is used **twice and large**, as an identity mark: slide 01 at 280px and slide 20
at 96px. The licence requires attribution wherever the mark appears, so every slide
that draws it carries the credit line. The per-machine golems
are drawn diamonds instead: a dense filled silhouette among the drawn line marks
reads as a different medium, and blobs below about 40px.

### The running order is Dr. Dub's, and the last nine slides are an appendix

Dr. Dub set the order the golem deck runs in. The fleet sequence, golem's own
mechanism, the caveats and the ask run first, at 04 to 38, with the two December
close-ups that survive in the main run at 09 and 10. The ladder, the six-layer
stack, what orchestration means, what buying it would cover, Ansible's coverage,
December's owners, the move that took four hand-ordered steps and *Where it
broke* run last, at 39 to 47.

**Nothing was cut to produce that tail.** Those nine stay in the deck as an
appendix the speaker drops live, so a slide that does not fit the clock is skipped
rather than deleted. `SPEC.md` records the order slide by slide, and
`SLIDE_MODULE_NAMES` in `decks/golem/__init__.py` is where it is actually written
down.

### The deck opens on a title slide, and the title slide carries four things

01 is the golem emblem, the name, Dr. Dub's tagline and the emblem's credit line.
No agenda, no date and no venue. The credit line is on the slide because the mark
is, as the licence requires.

**This replaces an earlier entry that described three things**, when the slide
carried the emblem, a headline reading *golem — one program, written down as
state*, and the credit. Dr. Dub then wrote the tagline — *Your Debian fleet in
the state you want, from one typed program with an undo for all changes* — and it
went on the slide verbatim, broken at its two clause boundaries so it sets as
three claims rather than wherever the wrap falls.

The headline is `TITLE` imported from `decks/golem/__init__.py`, so the deck title
and the slide's headline stay one string. **`TITLE` is now the bare name `golem`.**
The same string is the combined deck's Excalidraw frame name, and the tagline is
92 characters — as a frame name it reads `01 · Your Debian fleet in the state you
want, from one typed program with an undo for all changes`, which is unreadable in
a frame list. The tagline is therefore slide-local, typed once in `s01_title.py`,
and nothing else reads it. `NAME` and `TITLE` are now the same string; they are
read for different purposes — `NAME` for the output directory, `TITLE` for the
frame name — and nothing asserts they differ.

### The grey on *What you configure* means bought, not merely unreachable

The third legend swatch on slide 03 reads *you purchased, and can't configure*,
where it used to read *not yours to configure*. The wording is Dr. Dub's; an
earlier pass dropped the subject to fit the slide's 50-word ceiling in
`WORD_BUDGET_CEILINGS`, leaving a subjectless clause beside two sibling entries
that both lead with one. The ceiling went to 51 words, and the entry carries the
subject Dr. Dub gave it; it is 56 now, because the slide has since gained a fourth
legend entry — see *Two cells of slide 03 get a red X, and the rule that picks
them out*.

Slide 02's grey entry, *the provider operates it*, is deliberately unchanged. The
two slides ask different questions and do not share a legend.

### Two cells of slide 03 get a red X, and the rule that picks them out

Dr. Dub ruled on the open question this file used to carry. `Cluster membership`
under **Bare metal + config mgmt** and under **Docker (one host)** now draws
`icons.not_applicable`, a red X, over the grey, and the legend gains a fourth
entry: *this model has no cluster*.

**The rule is that the model has no cluster, so the row does not apply** — not
that lichess does not use it. Dr. Dub first described the cells as "the pieces
that lichess doesn't use", then extended the marking to the Docker column, where
lichess is not at all; if that were the rule the whole Docker column would be
marked. **Managed Kubernetes on the same row stays grey and unmarked**: the
provider runs the control plane, so *you purchased, and can't configure* is
literally true there. Every other grey cell was checked against that sentence and
fits, including `Hardware` across all six columns — a single host still sits on
purchased hardware, which is the structural difference from cluster membership.

The row and the two columns are found by `.index()` on the label tuples rather
than written as numbers, so reordering a row or a column moves the mark with it.
The mark is an X rather than a dashed outline because the deck already uses a red
dashed cell border for a losing move on the service-move slide, and the same red
in the same shape would read as the same claim.

### The caveats and the ask sit before the appendix

Either order was allowed. The caveats and the ask are main-run content — how
golem was built, where it stands, what is asked of the room — and the appendix is
what the speaker drops when the clock runs short. Cutting is easiest from the
tail, so the nine cuttable slides go last and a talk that drops them still ends on
the ask. Putting the ask behind nine droppable slides would bury the one slide
meant to stay on screen while people talk.

### The dense fleet has to *be* indistinguishable, not claim to be

Slide 15 says the eleven things the play added cannot be picked out of what the
machines already carry. If a viewer can find them, the slide argues the opposite
of what it says, so the drawing has to achieve it rather than assert it.

What achieves it is that **the module does not know which marks are the eleven**.
`s15_what_do_we_undo.py` draws one lattice per machine from one loop, one tone,
one stroke width and one mark size; the number of marks on a box is
`FEWEST_MARKS + sum(host.name.encode()) % 15`, deterministic, from the host's name
alone and never from its unit counts. It imports two scalars from `playbook` —
`CHANGES_MADE` and `HOSTS_CHANGED` — and never `STEPS`, so no code path could
single out an added mark even by accident. A later edit that "helpfully"
highlighted them would break the slide's argument, which is why the module says so
at the loop.

Two treatments were tried and rejected on the way. One filled block per machine
fails because **indistinguishability is a property of a population**: one mark per
box gives nothing to hide in, and coming off frame 14, where individual marks have
just appeared, a featureless slab reads as detail being taken away. Filling every
box to the same count reasserts a capacity and invites a count of it. The shipped
lattice varies 36 to 50 marks per machine and is ragged along its last row for
that reason, and the nine machines the play touched average slightly *fewer* marks
than the twenty-one it did not.

### The five goals are one list, and a test keeps the two slides in step

Slide 16 states them and slides 36 and 37 grade them, so the strings live in
`decks/golem/goals.py` and neither slide types a goal. This is the drift guard
`decks/vocabulary.py` already gives the five orchestration job names, applied
inside one deck instead of across two — and it is in `decks/golem/` rather than in
`vocabulary.py` for exactly that reason: `vocabulary.py` is scoped to strings both
decks must agree on, and only the golem deck reads the goals.

Four tests in `test_scenes.py` hold it together: the goals slide states every
`Goal.statement`; the scorecard marks every `GRADED_CLAIMS` entry exactly once
across the two documents; the scorecard's rows are the module's claims in order;
and the two slides' slices reassemble `scorecard.ROWS` exactly, so the split
cannot drop a row or draw one twice.

**What the module does not protect against.** For goals 1, 2 and 5 drift is
impossible by construction: `graded_claims` falls back to the statement with its
full stop stripped, so there is no second string to edit. Goals 3 and 4 keep
explicit claim tuples, because *Easier to plan / be certain things will work.* is
genuinely restated as *Easier to plan* and *Being certain a change will work*, and
no test can assert a relation between the two. Rewording one of those two
statements alone therefore leaves its claims stale. Dropping or renaming a claim
is still caught.

### Three mark states on the scorecard, and colour is not one of them

Green and red both read as the same grey at the back of a room, so the three
states differ by **silhouette**: `achieved` is a filled circle with a white check,
`qualified` a filled diamond with a white exclamation, `not_achieved` a
sharp-cornered filled square with a white minus bar. The outline and the interior
glyph each carry the state on their own, so colour is redundant rather than
load-bearing, and the three still separate in a greyscale crop of the mark column.

Two states were not enough. Three of the seven graded rows are qualified, and
grading any of them achieved or not achieved would have been false in one
direction or the other. `not_achieved` is deliberately not `not_applicable`, the
red X on slide 03: that mark means the claim does not apply, this one means the
claim was graded and failed.

### What the new slides added to the vocabulary, and why it was short

Five marks and five helpers, each because nothing existing said the thing:

- **`icons.not_applicable`** — the catalogue had no cross of any kind, and slide
  03 needed a mark rather than a fourth tone.
- **`icons.person`** — no human figure existed at all. Slide 32's review row needs
  a subject in the filled slot so the empty slot beside it reads as *nobody*
  rather than as *nothing drawn yet*.
- **`icons.achieved` / `qualified` / `not_achieved`** — see above.
- **`layout.legend(marks=...)`** — a legend swatch can now carry an icon overlay,
  so slide 03's fourth entry is a small copy of the cell it describes rather than
  a new colour. All four prior call sites keep their old signature.
- **`ansible_play.draw_play(step_states=...)`** — a step can be current, taken or
  not yet. An empty `step_states` still draws every step as taken, which is what
  the two prior callers relied on, and both rebuild byte-identical.
- **`layout.panel_height_for` and `layout.labelled_box_height_for`** — a panel can
  be sized to its content instead of to a guessed constant. Proved output-neutral
  by building the whole tree with and without them.
- **`machines.swatch_entry`** — the private swatch helper became public so slides
  11 to 14 could draw one legend entry without duplicating its geometry.

The icon sheet outgrew its grid at 26 marks, so `icon_sheet.COLUMNS` went 5 to 6.

### golem keeps the site that serves its own documentation — from another repository

The claim survived checking, with a correction to *where*. golem's own repository
builds and publishes `ghcr.io/dull-ca/golem-docs` and **does not deploy it**. The
Emet program that puts it on a host lives in the sibling repository `dulliac`
(`fleet/main.emet`, `fleet/sites/Sites.emet`, `fleet/sites/StaticSite.emet`),
which is golem's first outside consumer and takes golem's shared Emet libraries as
a flake input. What golem keeps is the site — a quadlet container unit, a systemd
service and an ingress route on `dull-01` — not the whole machine, which is why
slide 33's panel is headed *The site that serves golem's documentation*.

`examples/website/website.emet` in golem's own repository is **not** the source
for any of this: it declares `scroll { name = "remora" }`, a local VM in the
self-hosted-CI demo loop.

## Corrections that changed the argument, not the wording

Four framing errors were caught in review. They matter more than any styling
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

### A caveat slide once proposed a fifth glyph kind

An early draft of *Longer-term goals* drew networking primitives as glyphs of
their own. That is the one extension the architecture rules out — there are four
reconciler-owned glyph kinds and richer shapes are Emet libraries that compile
down to them — and it was false as an absence besides: `Routing.Route`,
`Traefik.Ingress` and `Quadlet.Expose` are Emet types today.

The box that replaced it names a gap that sits **inside** the four-glyph model:
nothing type-checks the text a networking library renders, because every glyph
field unifies with `String` and `emet` links no config-file parser. The other two
boxes on that panel are the same shape of claim — no multi-line string literal, so
a config file is `String.join "\n"`; and no file ownership in the language, though
the wire model carries an owner and a group.

The lesson is the one this file keeps relearning in the other direction: a slide
that draws a *wanted* feature has to be checked against the architecture as well
as against the code, because a plausible-sounding gap can be a proposal to break
the model.

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

### Rollback on failure is automatic, and it is the default — reversing an earlier finding

A prior session recorded that `recover()` settles an interrupted attempt without
re-applying, and concluded from that that rollback was probably operator-triggered.
**Both halves were wrong**, and slide 37 grades this goal **achieved** on the
strength of the code:

- `apps/golemd/src/config.rs:100` — the default is
  `on_exhaust: OnExhaustConfig::Rollback`.
- `foreman.rs:1005-1009` — when retries are exhausted, `rollback_unit` reverses
  that unit's write-ahead-log steps last-in-first-out (`:1776-1789`).
- `recover()` (`:1794-1819`) **does re-apply**: `redrive_intended` re-runs every
  `Intended` step without a terminal outcome, and the whole interrupted attempt is
  then reversed and marked `RolledBack`. It runs at daemon construction, before
  every reconcile, and after a caught panic.
- `apps/golemctl/src/main.rs:15-112` has no rollback, revert or undo verb at all.
  Rolling back to a previous revision means re-applying the previous manifest as an
  ordinary forward apply.

The scope is the failing **leaf unit**, not the revision — a sibling unit's applied
glyphs stay committed. That is the designed failure-isolation boundary rather than
a shortfall. It fires only after retries exhaust, a scroll can opt out with
`policy = keep` (the one serving golem's documentation does), and each reverse is
best-effort.

### "Every step undoable" is qualified, not achieved — reversing what the deck's author believed

The mechanism is complete: `journal.rs:93-127` defines nine `Inverse` variants and
`reconcilers.rs:722-749` dispatches all nine, with no `todo!()` and no catch-all
arm, and every `apply_*` observes the host before it writes. Three holes stop it
being a plain yes, and slide 36 grades it **qualified**:

- **`lineInFile` does not round-trip.** `append_line` creates the file when it is
  absent while `RemoveLineInFile` rewrites it empty, leaving an empty file where
  none existed; and the trailing newline it adds is never removed.
- **Parent directories golem creates are not recorded.** `write_file_atomic`,
  `append_line` and the symlink arm all `create_dir_all`, and their inverses carry
  only the leaf path. Only the *directory* glyph records the components it created.
- **A failed reverse is logged, not retried.** `foreman.rs:1747-1749` logs
  `"rollback step failed"` and still marks the step `Reversed`.

Two smaller ones worth knowing: `aptPackage` reverses with `remove`, not `purge`,
and records no prior version, so there is no downgrade; and a `Remove` of a glyph
golem has no record for assumes golem created it.

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

### Two things the bar does not catch

Both were found the hard way, and both cost a round each.

**`build.py` does not prune `dist/`.** It overwrites the files it writes and
removes nothing else, and a slide's filename carries its number — so renumbering
the deck leaves the old `NN-slug.excalidraw` behind. `bun run check` reads every
file in `dist/`, not every file the build produced, so it will happily pass a
stale slide that no longer corresponds to any module. Run `rm -rf dist` before a
build you intend to check.

**The only overlap assertion is image-versus-text.** There is no text-versus-text
and no text-versus-shape check, so anything that collides on the canvas without
involving an image passes the whole suite. Three defects in one round were
invisible in the source and in the tests, and appeared only when the slide was
rasterised and looked at: a lattice of marks bursting through a machine's border,
which showed only in a crop at full resolution; a sentence whose referent had
moved to the far side of its row; and a panel with ninety points of empty space in
it. **Render every new or changed slide and look at it.**

## The orchestration deck's audience is the constraint on it

The golem deck can assume interest. This one is for a room that is not sure what
golem or Emet are, so **a slide that is mostly sentences is wrong for it**. The
round that rebuilt its ending replaced two panels of prose with seven slides, and
the rule that produced them is: draw the relationship, do not state it.

Three consequences worth keeping:

- **Answer a list on the slide after you name it.** 07 names the five jobs; 08,
  09 and 10 repeat 07's boxes at 07's geometry with an answer on each row. The
  repeated form is the point — the second slide reads as the first one answered,
  and the third and fourth read as the same slide at a later date.
- **Steal a form from the other deck rather than inventing one.** The new stack is
  the golem deck's layered-stack form; the buy-versus-configure tones are the ones
  its two matrices already use; the by-hand chips are the fleet frames'
  notation. A newcomer learns one notation, not four.
- **Where both decks need a figure, it moves up, it does not get copied.** The
  thirty hosts and the machine box now live in `decks/`, not in `decks/golem/`. The
  extraction was verified by building the tree before and after and diffing: no
  slide changed.

## One list answered three times, and what the notation had to carry

Slide 08 is now the first of three states — today, the plan lichess drew up in
December, and with golem. The three share `decks/orchestration/job_answers.py`,
which takes the answers and no geometry, so the only thing that moves between
them is the right-hand column.

### A tool that enacts a placement is not a tool that chooses one

This is the round's whole accuracy risk, and it is a repeat of one already
recorded above: **manual placement is a choice, not a defect**, and golem is
deliberately not a scheduler. Slide 26 draws placement in the by-hand notation,
so a slide 10 that handed the placement row to golem would contradict the deck
sixteen slides later, and would restate the error an earlier golem deck was
rewritten to remove.

A single chip cannot say *a person decided this and the tool carried it out*, so
the notation grew a second relation. Chips separated by a gap are a mix — health
is by hand, monitoring and systemd, and means all three. Chips joined by an arrow
are a decision and its enactment, and **each chip carries the half it does**:
`by hand / chooses the host` → `golem / installs it there`. The by-hand mark is
on placement in all three states; only the enactor changes. The two small labels
are what make the pair unmisreadable by someone who sees slide 10 and no other,
and they are the reason the arrow needs no legend entry.

### Check what golem does at runtime before drawing it beside systemd

Lifecycle on 10 is **golem and systemd, as the same pair**, because
`apply_systemd` shells out — `systemctl daemon-reload`, then `systemctl enable
--now`, with a `systemctl start` fallback for generated units
(`apps/golemd/src/reconcilers.rs`). golem sets a unit's state through systemd;
systemd is what runs it and what restarts it. Drawing golem in systemd's place
would have been false, and the honest answer was two chips rather than one.

Health is the same question answered the other way: golem gets **no chip** on the
reconciliation row, on 10 or anywhere, because drift is reported and never
corrected. A green chip there is *The claims 25 and 26 must not make* in another
form.

### The row that never changes hands

All three states carry a configuration-management row, and Ansible holds it in
every one. It is not one of the five jobs, so it is drawn outside them — below a
rule, unnumbered, unboxed, its label in the recessive tone, its chip still in the
answer column so it stays comparable. Four signals and no caption, because a
caption saying "this one is different" is the thing a drawing should not need.

That row is also the sequence's quiet argument: the part nobody proposes to
replace is the part that never moves.

### The bottom of the canvas was already full

The five boxes are slide 07's geometry and cannot move without breaking the
07 → 08 flip, which left 92px between the last box and the bottom margin — a rule
and one row, or a row and the legend, but not both. The legend moved into the
header, right-aligned beside the title, on all three. Worth knowing before anyone
tries to add a seventh row: there is no room, and taking it from the five boxes
breaks the one thing that makes 07 through 10 read as one slide.

## Promise theory is right about golem's shape and wrong about its schedule

The framing invites four sentences that the code does not support, and the fifth
round's whole accuracy risk was writing one of them onto a slide. What is true:

- **No host acts on another.** golemd has no outbound HTTP client at all.
- **No central controller decides.** `golemctl` ships manifest bytes; each daemon
  picks its own scroll and diffs it against its own journal.
- **golem reverses only what it recorded**, so it never removes something it did
  not add.

What is false, and is listed with its evidence under *The claims 25 and 26 must
not make* in `SPEC.md`: continuous convergence, self-healing, eventual
consistency, and a controller that computes each host's work. golemd has no
timer, no watcher and no loop; a reconcile happens only when something POSTs
`/manifest`; and drift is reported by an opt-in host-reading plan and never
corrected. Peer gossip is ADR 0039's design with no code behind it.

**Declarative-on-demand is still a sharp contrast with Ansible**, and it is the
one the deck draws: an ordered list of steps run from a controller, against a
host handed the state it should be in and left to work out its own. Slide 25
exists to say the schedule out loud, because slide 24 would otherwise be read as
promising a loop.

The consequence this framing earns: **golem does not sequence a cross-host move**
because no agent can promise on another's behalf. On the golem deck that fact is a
caveat under slide 21; here it falls out of the model, and slide 24 draws it that
way.

## The machine-lifecycle deck: a shape carries the argument

### Draw the figure twice and change only one channel

The claim is that four of five steps are done by hand and the tool covers the one
in the middle. That is a **shape**, and a shape is read faster than it is argued.
Slides 01 and 05 draw the same five steps, the same marks and the same geometry,
and differ in the three spans underneath: *by hand · Ansible · by hand* becomes
*Pulumi · Ansible · golem*. The spans keep their widths, so the eye reads which
stretches changed and which did not.

This is the orchestration deck's "answer a list on the slide after you name it"
applied to a figure rather than a list, and it is why every coordinate lives in
`lifecycle.py` and neither slide passes geometry.

### Verify the tooling claim before drawing it, and expect the verdict to move

The brief flagged step 1 — ordering a machine — as the weak claim, on the belief
that OVHcloud's order-cart API was not cleanly wrapped. Reading the provider's own
reference reversed that: **`ovh.Dedicated.Server` orders bare metal**, and its
docs open with "Use this resource to order and manage a dedicated server". All
three of steps 1 to 3 are that one resource.

The caveats worth saying out loud turned out to be different ones, and slide 08
is where they go: the `order_cart` family exists only as **data sources**, so a
person still picks the plan; the order is **asynchronous**, and the provider gives
up waiting after two hours while OVHcloud goes on delivering; and partitioning
moved onto the server resource in **provider v2.0.0**, which removed the
installation-template resources anyone with older knowledge will reach for.

The general lesson is the one the earlier rounds kept learning about golem's own
code: **the checkable claim is the one to check, and checking it usually changes
what the slide should say** — here in the direction of a stronger claim with
sharper limits, rather than a weaker one.

### A tool gets explained where it appears, not in a block of explanations

The obvious structure was a block of three slides — what Pulumi is, what Ansible
is, what golem is. Ansible's would have repeated slide 03, which already has to
draw a play and a host because step 4 is the one step a tool owns today. So
Ansible's definition sits on 03, where it is simultaneously the truth about today
and the introduction, and the block has two slides in it. **A slide that would be
a second telling is a slide that should not exist.**

### An absent artifact is drawn absent, not described as missing

Slide 10 draws artifact, arrow, machine twice. On the left the artifact slot is
empty — dotted `INK_GHOST`, the notation the fleet frames already use for a thing
that is not there yet. A sentence saying there is no file today would be arguable
in the room; an empty box beside a full one is not.

### A mark takes one tone

`icon_card_row` passes a card's `icon_tone` and nothing else, so a composite mark
whose second tone is a hard-coded default cannot be recoloured by the slide that
draws it. The first version of `os_install` drew violet slabs on a red card, and
`disk_layout` distinguished allocated from free space by a fill, which vanishes
under any tone whose fill is white. Both now say what they mean with one tone —
`disk_layout` divides by unequal widths, `os_install` defaults its payload to the
tone it was given.

### An image may not sit on drawn text, and a test says so

The golem emblem on slide 20 was drawn at x 1180 while the legend's last caption
runs to x 1291, so the robot stood on "nobody has it written down" in every
render. The emblem moved to the bottom-right corner and the credit to the strip
under the legend.

The check that now guards it measures a text element by its **widest set line**,
not by its declared box: a title given the full content width would otherwise
forbid drawing anything to the right of it. Two rounds looked at that slide
without seeing this, which is the argument for the assertion over the eye.

## Questions closed

- **What the grey means on two cells of slide 03.** *Asked* because the legend
  reads *you purchased, and can't configure*, and `Cluster membership` under
  *Bare metal + config mgmt* and under *Docker (one host)* is grey for a different
  reason: there is no cluster in either model, so nothing was bought. `CELL_TONES`
  was left alone at the time and the mismatch recorded here. **Closed by Dr. Dub's
  ruling:** those two cells now carry a red X and the legend a fourth entry. See
  *Two cells of slide 03 get a red X, and the rule that picks them out*.
- **What language Emet actually resembles.** *Asked* because the original
  description, "inspired by (nearly identical to) emet", is self-referential, and
  a later draft's "ML-family" was plausible but unverified. **Closed by Dr. Dub:
  Emet is Elm-like**, and the claim was then checked against `apps/emet/`.
  `apps/emet/CLAUDE.md` describes the language as modeled on Elm; what holds is
  the offside layout rule, Hindley-Milner inference with let-generalization,
  compile-time `case` exhaustiveness and redundancy, row-polymorphic records with
  `{ r | f = v }` update, **Elm's operator fixity table exactly**, Elm's three
  constrained type variables (`number`, `comparable`, `appendable`) with no user
  typeclasses, and Elm's module system and stdlib surface down to the omissions —
  no `List.head`, and `String.toInt : String -> Maybe Int`.

  The differences worth saying out loud rather than hiding: no `|>` and no
  user-defined operators at all; no `type alias`; not total, since ADR 0011
  relaxed it and the backstop is a 20,000-frame depth counter; `Secretspec.get` is
  typed `String -> String` but reads a real secret provider at compile time, so
  secret-ness is a dynamic taint rather than a type; and no `Result`, `Dict`,
  `Set`, `Task`, `Cmd`, no ports and no runtime, because the compiler evaluates
  the whole program and the output is data.

  The deck says it **once**, in slide 27's subtitle — *Emet is an Elm-like
  language: a program is typed and functional, and evaluates to a list of scrolls*
  — and states only the hedged headline, because each specific similarity is a
  claim needing its own defence in a room that may contain an Elm user. It is
  deliberately not said on the ask slide, where question 4 asks the room to learn
  Emet and nothing on that slide may need explaining, and not on the slides that
  name a file or a binary rather than describe the language.
- **Whether "workloads, quadlets and ingress are Emet libraries that compile down
  to the four glyphs"** holds — slide 27's callout, asserted in `CLAUDE.md` and
  previously unchecked against Emet source. **Closed against `lib/`:** a
  `Quadlet.ContainerUnit` emits `aptPackage { name = "podman" }`, a `file` at
  `/etc/containers/systemd/<name>.container` and a `systemdService`
  (`lib/Quadlet.emet:495-509`); Traefik's ingress emits `aptPackage`,
  `systemdService` and `file` (`lib/Traefik.emet:164-196`); and `Routing.Route` is
  an ordinary Emet record that bottoms out in file contents
  (`lib/Routing.emet:1-11`). No library introduces a fifth glyph kind.

## Open questions

- **Whether the inventory-derived numbers above are right.** They come from a
  hand-written indent parser, since `yaml` is not in the golem devenv.
- **Five things verified only by reasoning:** Excalidraw's real dotted stroke (the
  check renderer approximates it with a dash array), roughness (the editor draws
  sketchier strokes than the check renders), the embedded emblem (rasterised only
  through the local jsdom harness, which carries a workaround for a librsvg
  percentage-sizing bug on embedded images, and never through Excalidraw itself),
  projector legibility (judged from 1600px renders and 700px downscales, not a
  projector), and the handover arrow's head (the local renderer draws no
  arrowheads, so a 46px arrow beside a 46px chip has been checked as an element and
  not as a picture).
- **What December's plan actually said.** Placement by Ansible, and plumbing by
  Ansible with dnsmasq and SRV records, come from Dr. Dub's own table for the
  orchestration deck's slide 09 and were not read out of any lichess source. The
  golem deck's slides 09, 10 and 46 describe December as it *ran* — `hosts.py`,
  generated config, quadlets — so the two are only consistent if the plan is a
  thing distinct from what was running.
  Slide 09's title says "planned" for that reason.
- **Whether lichess would keep `site.yml` as the play's name** on the lifecycle
  deck's slide 03. The six basics are the brief's own list; the filename is the
  orchestration deck's placeholder carried over.
- **Whether the Pulumi provider's ordering path has been exercised against
  OVHcloud.** The resource, its fields and the two-hour wait are documented; that
  the deck's claim is *the documentation says so* is a weaker claim than *we have
  ordered a machine this way*, and the difference is worth stating out loud.
- **Whether "better templating" has a checkable shape beyond the missing
  multi-line string literal.** Slide 34 draws the one limit that can be cited —
  a raw newline before a closing quote is an "unterminated string literal", so
  every config file in `lib/` is a `String.join "\n"` — and says nothing about what
  a templating feature would be.
