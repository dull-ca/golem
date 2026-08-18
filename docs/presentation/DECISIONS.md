# Presentation — what we decided, and why

A design log for the decks under `docs/presentation/`. `SPEC.md` records what
each slide says; this records the decisions behind them, including the ones we got
wrong first and had to reverse. It exists so a later session — or a later
conversation with Dr. Dub — starts from the conclusions rather than re-deriving them.

Seven rounds of review produced everything below. Where a decision replaced an earlier
one, both are here; the reversals are the most useful part of the file.

## What exists

Three decks, generated from Python, never hand-authored as JSON.

- **`decks/golem/`** — 34 slides. A title slide, the talk — what problem golem
  solves, how it works, how to use it — and a nine-slide appendix.
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
29 → 33 slides. The thirty-fourth is the title slide, which is not a split. Tests
assert the floor and the budget, so violating it fails the build rather than being a
matter of taste.

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

Slide 28's numbering and tones are the spine: 1 core OS/network/security (slate),
2 application hosting (teal), 3 connective infrastructure (blue), 4 tools,
dependencies, runtimes (violet), 5 the applications (green), 6 lifecycle/schedule/
scaling (orange). Everything downstream imports those constants rather than matching
by eye.

### Layers are categories of work, not strata inside a machine

The correction that made the fleet sequence work. The first version drew each machine
as a miniature of slide 28, with the six layers stacked inside the box. That asserts a
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

Twenty-two marks in `icons.py`, no external assets. The exception is golem's own emblem —
**robot-golem by Lorc, game-icons.net, CC BY 3.0**, vendored under `assets/`, embedded
as a data URL so the build stays offline, credited in four places.

It is used **twice and large**, as an identity mark: slide 01 at 280px and slide 14
at 96px. The licence requires attribution wherever the mark appears, so every slide
that draws it carries the credit line. The per-machine golems
are drawn diamonds instead: a dense filled silhouette among nineteen open line marks
reads as a different medium, and blobs below about 40px.

### The running order is Dr. Dub's, and the last nine slides are an appendix

Dr. Dub set the order the golem deck runs in. The fleet sequence and golem's own
mechanism run first, at 04 to 25, with the two December close-ups that survive in
the main run at 09 and 10. The ladder, the six-layer stack, what orchestration
means, what buying it would cover, Ansible's coverage, December's owners, the move
that took four hand-ordered steps and *Where it broke* run last, at 26 to 34.

**Nothing was cut to produce that tail.** Those nine stay in the deck as an
appendix the speaker drops live, so a slide that does not fit the clock is skipped
rather than deleted. `SPEC.md` records the order slide by slide, and
`SLIDE_MODULE_NAMES` in `decks/golem/__init__.py` is where it is actually written
down.

### The deck opens on a title slide, and the title slide carries three things

01 is the golem emblem, the deck title and the emblem's credit line. No subtitle,
no agenda, no thesis sentence, no date and no venue — the rule that a slide gets a
subtitle only when it says something the title does not leaves an opening slide
with nothing else to carry.

The headline is `TITLE` imported from `decks/golem/__init__.py`, so the deck title
and the slide's headline are one string and the combined-deck frame reads `01 ·
golem — one program, written down as state`. The credit line is on the slide
because the mark is, as the licence requires.

### The grey on *What you configure* means bought, not merely unreachable

The third legend swatch on slide 03 reads *you purchased, and can't configure*,
where it used to read *not yours to configure*. The wording is Dr. Dub's; an
earlier pass dropped the subject to fit the slide's 50-word ceiling in
`WORD_BUDGET_CEILINGS`, leaving a subjectless clause beside two sibling entries
that both lead with one. The ceiling is now 51 words, and the entry carries the
subject Dr. Dub gave it.

Slide 02's grey entry, *the provider operates it*, is deliberately unchanged. The
two slides ask different questions and do not share a legend.

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
caveat under slide 15; here it falls out of the model, and slide 24 draws it that
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

The golem emblem on slide 14 was drawn at x 1180 while the legend's last caption
runs to x 1291, so the robot stood on "nobody has it written down" in every
render. The emblem moved to the bottom-right corner and the credit to the strip
under the legend.

The check that now guards it measures a text element by its **widest set line**,
not by its declared box: a title given the full content width would otherwise
forbid drawing anything to the right of it. Two rounds looked at that slide
without seeing this, which is the argument for the assertion over the eye.

## Open questions

- **What the grey means on two cells of slide 03.** The legend now reads *you
  purchased, and can't configure*, and every grey cell was checked against it.
  `Cluster membership` under *Bare metal + config mgmt* and under *Docker (one
  host)* is grey because there is no cluster in either model, not because anything
  was bought. `CELL_TONES` in `decks/golem/s03_what_you_configure.py` was left as
  it is.
- **What language Emet actually resembles.** The original description, "inspired by
  (nearly identical to) emet", is self-referential. Nothing false is drawn — the slide
  says only that a typed, functional program evaluates to one Scroll per host — but the
  comparison needs an answer before it is made out loud.
- **Whether the inventory-derived numbers above are right.** They come from a
  hand-written indent parser, since `yaml` is not in the golem devenv.
- **Five things verified only by reasoning:** Excalidraw's real dotted stroke (the
  check renderer approximates it with a dash array), roughness (the editor draws
  sketchier strokes than the check renders), the embedded emblem (never rasterised),
  projector legibility (judged from 1600px renders, not a projector), and the
  handover arrow's head (the local renderer draws no arrowheads, so a 46px arrow
  beside a 46px chip has been checked as an element and not as a picture).
- **What December's plan actually said.** Placement by Ansible, and plumbing by
  Ansible with dnsmasq and SRV records, come from Dr. Dub's own table for the
  orchestration deck's slide 09 and were not read out of any lichess source. The
  golem deck's slides 09, 10 and 33 describe December as it *ran* — `hosts.py`,
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
- **Whether "workloads, quadlets and ingress are Emet libraries that compile down to
  the four glyphs"** holds. It is asserted in `CLAUDE.md` and was not confirmed against
  Emet source.
