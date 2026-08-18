# The talks

Three decks. **golem** runs in five parts — what problem golem solves, what was
wanted before it was built, how it works, where it falls short, and what is asked
of the room. **orchestration** is a standalone primer on
what a cluster does, built from icons, landing on the one job golem deliberately
does not do. **machine-lifecycle** is a short argument about the five steps that
bring one lichess machine into service, and which tool should take each.

Build and run instructions are in `README.md`; what follows is the argument, what
each slide has to say, and the format facts the generator depends on.

## One mode per slide

Every slide is marked **reference** or **explanation**, and holds to it. Neither
deck is a tutorial or a how-to, and nothing on a slide is phrased as instruction:
no "you should", no "note that".

A **reference** slide states what exists — flat, complete, no argument and no
commentary. A verb list is a verb list; a matrix is a matrix; a glyph card names
the glyph and nothing more. An **explanation** slide says how a piece relates to
the others and why it is that way. Explanation may make a claim, but the claim has
to be one a competent engineer could dispute on technical grounds. A sentence that
cannot be disagreed with is rhetoric, and does not go on a slide.

Two consequences that the shipped decks now obey, and that a future edit must not
undo:

**A slide gets a subtitle only when it says something the title does not, and a
closing line only when that line is a fact.** Most slides have neither. An earlier
draft gave all thirty-nine both, which produced seventy-eight aphorisms — "what
you stop operating, you start depending on", "the loop is the product" — none of
which defined anything. The slot was the defect, not the wording, so the slot is
gone.

**A short label is read cold, alone, out of order.** It has to be a noun phrase
that names what the box is, never a clause that leans on a sentence elsewhere on
the canvas. The hub of slide 42 of the golem deck once read `Layer 6 / one word` —
a fragment snapped off that slide's own subtitle — where it had to read
**Orchestration**.

The first time a term appears it gets one flat sentence saying what it is: an
image is read-only filesystem layers plus a config, named by the digest of its
contents; reconciliation compares the state you asked for against the state on the
host and acts on the difference. Not an evocation of it.

---

# The golem deck

## The argument

**The running order is Dr. Dub's.** The deck was reordered at his direction: the
fleet sequence and golem's own mechanism now run ahead of the lichess stack, the
December material and *Where it broke*, which run as an appendix at the end.
Every number below is the order as the deck runs today.

**01 is the title.** The golem emblem, the name, Dr. Dub's tagline under it, and
the emblem's credit line.

**One stack, and every hosting choice is a line drawn across it.** 02 and 03 set
that frame twice — first by what you *buy*, then by what you *configure* — so the
audience already has a shape in their head before any lichess detail arrives.
Both carry the **lichess is here** marker over their own column, and 39 plants it
a third time: on a ladder from machines to resources, lichess is on the leftmost
rung.

**04 to 15 are the fleet, and they are what the talk is for.** The sequence draws
the thirty machines the Ansible inventory actually names, and what changes between
frames is which units sit on them and who keeps each one.

**09 and 10 are December in detail**, and they sit inside the sequence. 09 is
service discovery, 10 is placement and lifecycle. They exist so that what December
lacked is about structure rather than about a stack nobody has seen.

**11 to 15 are what a playbook leaves behind.** 11 to 14 page one Ansible play
through the same thirty machines, one step per frame, each step naming a host
group and each frame keeping what the frames before it put down. 15 draws the
same thirty machines packed with the state they already carry, and asks what
there is to undo. Between them they draw the first two of the five problems 47
states in words: a change is a mutation applied to a group of hosts, and once it
has run nothing on the host says which marks the play added.

Every step gets its own frame. Nothing in the sequence asks a viewer to notice a
difference between two slides, and nothing depends on a transition — see
*Excalidraw+ transitions are unverified*.

**16 states the five goals**, in Dr. Dub's own words and in the order he wrote
them. It sits between the problem and golem, and 36 and 37 grade the same five
from the same module, so the two ends of the deck read from one list — see *The
goals are one list, stated once and graded once*.

**17 to 21 are golem arriving on the same thirty machines**, and they end the
fleet sequence: the compile, the dispatch, the fleet mid-apply, the fleet
converged, and one service moving between two hosts.

**A person choosing the host was never the defect.** golem keeps that choice — 22
and 23 here, and 26 of the orchestration deck, all say so — so no slide may frame
manual placement as a deficiency golem outgrew. What December lacked was
mechanism: no drain, no rollback, no preview, and no way to say *this service now
runs on host B* as one change. 21 names that last one and only that one.

**22 and 23 are the pivot.** 22 is what golem is and is not; 23 pairs each of
seven requirements with the property that meets it. The five problems those rows
answer are on 47. 23 used to carry a subtitle announcing how to read the table,
which is the speaker's job, so the subtitle is gone.

**24 to 31 are the "how it works" third.** 24 is the pipeline end to end, 25 the
diff, 26 apply and undo. 27 and 28 are the authoring contract — a typed
functional program, and exactly four glyph kinds. 29, 30 and 31 are the
operator's view: two binaries, one wire, and plan-before-apply.

**32 to 35 are the caveats.** 32 is how golem was built, 33 where it stands
today, 34 what the language does not do yet, and 35 what the reconcilers do when
the host fights back. They state limits rather than claims, and each one is
drawn from the code it describes.

**36 and 37 grade the goals 16 stated**, one row per graded claim, each carrying
a verdict and the evidence for it. 38 is the ask: four questions in Dr. Dub's
words, and the slide that stays on screen while the room talks.

**39 to 47 are an appendix.** They are kept in the deck for the speaker to cut
live rather than dropped from it, and nothing that was in the deck before the
reorder was removed. 39 and 40 are the ladder from machines to platforms, with
Portainer added on 40; 41 is the six-layer lichess figure and 42 expands its layer
6; 43 recolours 41; 44 measures Ansible's coverage; 45 names December's owners; 46
is the move that took four hand-ordered steps; 47 is the five problems.

**The caveats and the ask sit before the appendix, not after it.** 32 to 38 are
main-run content — how golem was built, where it stands, what is asked of the
room — and the appendix is what the speaker drops when the clock runs short.
Cutting is easiest from the tail, so the nine cuttable slides go last and a talk
that drops them still ends on the ask.

**40 is the same ladder with Portainer added, and nothing else changed.**
Portainer used to share 39, which overstated it: it is a web UI on one machine out
of thirty, not a rung and not a layer. Splitting it lets 39 make the claim about
lichess cleanly, and lets 40 state Portainer's scale — one box, drawn as one mark
in a row of thirty.

**41 is the six-layer lichess figure.** Layer 6 is drawn as a column beside bands
2–5 rather than a band on top, because orchestration acts *across* those layers.
That drawing decision is itself an argument, and 42 collects on it: layer 6
expanded is five separate jobs, each one done by a platform, by a script, or by a
person.

**The figure is shown twice, not four times.** 43 recolours it to show what buying
orchestration would cover — the yardstick. That is the only recolour, and it is
drawn at geometry identical to 41, so flipping between the two changes colour and
nothing else. Where we were is then shown by *different* forms, because four
recolourings of one figure read as one slide shown four times: 44 measures
Ansible's coverage as bars, layer by layer, and 45 names the four tools that
shared December's layers and the one job nobody took.

## The six-layer palette is the deck's vocabulary

The tones on 41 are named once, in `DESCRIPTIVE_LAYER_TONES` in
`decks/golem/lichess_stack.py`, and every slide that speaks about layers imports
them rather than matching by eye:

| | layer | tone |
|---|---|---|
| 1 | Core OS, network, security | `SLATE` / `SLATE_FILL` |
| 2 | Application hosting | `TEAL` / `TEAL_FILL` |
| 3 | Connective infrastructure | `BLUE` / `BLUE_FILL` |
| 4 | Tools, dependencies, runtimes | `VIOLET` / `VIOLET_FILL` |
| 5 | The applications | `GREEN` / `GREEN_FILL` |
| 6 | Lifecycle / schedule / scaling | `ORANGE` / `ORANGE_FILL` |

**A layer is a kind of work, not a compartment inside a host.** Layer 1 is the
work of configuring the core OS, network and security; layer 5 is the work of
running the applications. In the fleet sequence the six tones therefore ride on
the tool chips and the arrows — the thing doing the work — and never on the
machines, which hold their units instead. 40's Portainer bar takes the layer-6
tone from the same table for the same reason: Portainer's job is layer-6 work, on
one machine.

An earlier draft drew every machine as a miniature of the slide-41 figure, six
bands to a box. That asserts a structure that is not there, and it collapses two
independent facts into one channel: which machines a tool reaches, and what kinds
of work it does there. Those are separate axes now, which is what lets one frame
say Ansible touches all thirty machines and keeps the units on eight of them.

A near-miss would break this, so no slide picks a hex or a `Tone` of its own for
a layer. The ladder rungs on 39 and 40 keep `YOURS` and `PLATFORM`, because that
axis is who picks the machine, not which layer is meant.

## The forty-seven slides

Frame names in `dist/golem/golem-deck.excalidraw` are `NN · Title`; filenames are
`NN-slug.excalidraw`. Both derive from position in `SLIDE_MODULE_NAMES` in
`decks/golem/__init__.py`.

### 01 · golem — `s01_title.py` — *title card* — **reference**

Four elements: the emblem at 280px centred, `TITLE` at `TITLE_SIZE` centred under
it, the tagline at `HEADING_SIZE` in `INK_SOFT` below that, and
`golem_symbol.CREDIT` at `CAPTION_SIZE` in `INK_FAINT` at the foot. No agenda, no
date and no venue.

The tagline is Dr. Dub's sentence, drawn as one `note()` with hard newlines at
its two clause boundaries so it sets as three lines rather than wherever the wrap
falls:

```
Your Debian fleet in the state you want,
from one typed program
with an undo for all changes
```

`TITLE` is imported from `decks/golem/__init__.py` rather than retyped, so the
deck title and the headline on this slide are one string. It holds the bare name,
because the same string is the combined deck's Excalidraw frame name and the
frame list reads `01 · golem`; the tagline is 92 characters set on one line
there, which is unreadable in a frame list. The
emblem's credit line is on the slide because the licence requires attribution
wherever the mark appears — see *The imported mark*.

### 02 · What you buy — `s02_what_you_buy.py` — *matrix* — **reference**

A service-model responsibility matrix, deliberately vendor-neutral — no AWS, GCP
or Azure. Rows top to bottom: Data, Application, Runtime & middleware,
Virtualisation, Operating system, Network & storage, Hardware, Facility & power.
Columns:
Own hardware, Colocation, Rented bare metal, IaaS (cloud VMs), PaaS, SaaS.

**Virtualisation sits above Operating system, not below it.** There is a system
on the metal first and a hypervisor on top of it; anything inside that hypervisor
is a guest, and the guest is the less interesting of the two for this talk. The
order also puts 02 in step with 03, where "Container runtime" already sits above
"Host OS & kernel".

Cells staircase down the columns, and each depth is derived from what the model
sells rather than from the column beside it: own hardware all eight, colocation
seven, rented bare metal the top five — the provider hands over a machine and you
install the system and any virtualisation on it — IaaS the top three, because the
provider runs the machine's system *and* the hypervisor and what you get is a
guest, PaaS the top two, SaaS none. The exception is SaaS Data, which is hosted.

IaaS is the only depth the row swap moved, 4 to 3, and it is the one that can be
misread. The note under the legend is what stops it: the row is the system on the
metal, and the guest inside the provider's virtualisation is yours. Without that
line the slide appears to claim the provider patches your VM. Do not drop it and
leave the depth at 3, and do not raise the depth to 4 — that would put a yours
cell under a theirs cell and break the staircase the legend explains.

Otherwise cells carry colour and no text. An earlier draft wrote `YOURS` /
`THEIRS` into all forty-eight of them; at the type floor that is forty-eight words
of visual noise saying what three legend swatches say once. The legend is the rest
of the prose: *you operate it* / *the provider operates it* / *yours, stored by
the provider*.

A **lichess is here** badge sits over the *Rented bare metal* column. It has no
connector down to the header, unlike 26's: the matrix header wraps to two and
three lines, and a dashed line long enough to be legible ended inside the words.
The badge is centred on the column, which is enough.

### 03 · What you configure — `s03_what_you_configure.py` — *matrix* — **reference**

The same matrix shape, a different question, and the second and last matrix in
this deck. Columns: Bare metal + config mgmt, Docker (one host), Swarm, Nomad,
Kubernetes, Managed Kubernetes. Rows: App config & secrets, Scaling policy,
Service discovery & load balancing, Scheduling & placement, Cluster membership,
Container runtime, Host OS & kernel, Hardware. Three tones and one mark, and again
the only prose on the slide: *you configure it* / *the platform provides it* /
*you purchased, and can't configure* / *this model has no cluster*. A **lichess is
here** badge sits over *Bare metal + config mgmt*, the same treatment as 02 and,
for the same reason, drawn without a connector.

Two cells on the **Cluster membership** row carry `icons.not_applicable`, a red X
over the grey: *Bare metal + config mgmt* and *Docker (one host)*. Neither model
has a cluster, so the row does not apply there, which is a different claim from
the one the grey legend entry makes. Managed Kubernetes on the same row keeps the
plain grey, because a provider-run control plane makes *purchased, and can't
configure* literally true. Both the row and the two columns are found by
`.index()` on the label tuples in `s03_what_you_configure.py`, so reordering a row
or a column moves the mark with it.

## 04 to 15 · The fleet, and the playbook run across it — `decks/machines.py`, `decks/lichess_fleet.py`, `decks/golem/fleet.py`

Ten frames over the machines lichess actually has, running 04 to 08 and 11 to 15,
with the two December close-ups at 09 and 10 between them. 19 and 20, in the next
section, are the same thirty boxes again once golem has arrived.
`decks/lichess_fleet.py` carries the thirty hosts named in
`lichess-sysadmin/ansible/inventory/hosts.yaml`, and for each the number of units
on it split by the inventory's own `managed` field. Names and counts only: no
addresses, keys or tokens go near a projected slide.

The data and the machine box sit **above both decks**, in `decks/`, because the
orchestration deck draws the same thirty machines on its slide 22.
`decks/machines.py` holds the box, the cell grid, the unit legend, the scroll
mark and the wide-frame geometry; `decks/golem/fleet.py` holds what only the
golem deck means by them — the six kinds of work, the tool chips, and the
per-frame states the sequence steps through. A second copy would drift from the
inventory the first was read from.

**`managed: false` means Ansible does not touch that unit**, and the field
defaults to true, so an entry without it is Ansible's. Sixty of the eighty-two
units carry `managed: false`. Eight hosts have at least one unit a tool keeps —
achoo, apate, cobar, dingo, lucid, orbit, radio, thonk — and the other twenty-two
are kept by hand. That ratio is the sequence's whole argument for item after item,
and it is read off the inventory rather than invented.

The counts merge each host's entry under `all` with the same host's entry under
every group that adds units to it; the `mongodb` group in particular carries
`databases` blocks that `all` does not. Reading `all` alone finds forty-two
unmanaged units instead of sixty.

### What a machine box holds

**Units, not layers.** A box carries the host's name in mono at the top and one
cell per unit below it, in a four-by-three grid — twelve slots, and `talos` at
eleven is the fullest host in the fleet. Two channels, independent on purpose:

- the **border** names the tool that has done machine-level work on that host —
  Ansible's grape once 05 has run, golem's green on the hosts golem keeps
- a **cell** is filled in the tone of the tool that keeps that unit, or drawn as a
  red dashed outline when a person keeps it

A host with no tool-kept unit and at most one unit recorded anywhere gets a red
**?** across its free slots: nothing a tool knows, and next to nothing written
down. Ten hosts qualify — feck1, feck2, gappa, kaiju, krakn, pingu, scaly, sofia,
syrup, taffy — and `scaly` records no unit at all.

Down the left run the six kinds of work with their slide-41 tones, then the tool
chips. A chip carries the tool's name, one line saying what it does, and a row of
numbered swatches for the kinds of work it performs. `tool_column` raises if a
chip's gloss runs past two lines rather than letting it collide with the swatches.

11 to 15 draw neither the work key nor the three-entry unit legend: they call
`machines.draw_fleet` directly rather than `fleet.draw`, because both of those
speak about the whole fleet and these frames speak only about what one play put
down. 11 to 14 carry a single swatch entry saying what a cell is on them; 15 says
what a mark is in its subtitle and carries no legend at all.

### The empty state recedes

An unconfigured machine is drawn `INK_GHOST` and **dotted**, both lighter than
anything else on the canvas. Faint and dotted means *not yet* wherever it appears,
so the eye lands on what a tool has done rather than on what it has not. The
earlier treatment — a pale slate fill behind a solid outline — made frames 04 and
05 indistinguishable across a room, which is the entire delta of 05.

### Geometry

Thirty machines, six across and five down, at 163 × 118. That count and that size
are the balance point between two demands that pull apart: enough boxes to say
*this is a lot*, and boxes big enough that a viewer can count the units inside
one. Thirty at 6 × 5 keeps a unit cell at roughly 35 × 23, which reads; a sixth row
would not. The wide frames share one geometry and it is not a parameter — a box
that moved between them would read as a different fleet.

Three frames step out of the wide shot deliberately. 17 and 18 are the compiler
and the dispatch, which happen off the fleet or one host at a time; 21 is a
two-machine story that twenty-eight unchanging boxes would swamp. Each says on the
slide that it is showing part of the whole.

### 04 · The fleet: thirty machines — `s04_fleet_machines.py` — *machine fleet* — **reference**

Rented bare metal, named. Every box is empty and dotted.

### 05 · The fleet: Ansible does the basics — `s05_fleet_basics.py` — *machine fleet* — **explanation**

Every border turns Ansible's grape: the core OS, the private network and security,
on all thirty. The boxes stay empty, because nothing runs on them yet. One chip,
tagged with kind of work 1, and a connector into the fleet.

### 06 · The fleet: the rest by hand — `s06_fleet_by_hand.py` — *machine fleet* — **explanation**

Eighty-two units appear, all of them red and dashed, and ten hosts carry a **?**.
The unevenness is the frame: a host holds between zero and eleven units and no two
hosts hold the same number. Chips: **Ansible** on work 1, **By hand** on 2 to 6.

### 07 · December: the config is generated — `s07_fleet_generated_config.py` — *machine fleet* — **explanation**

`hosts.py` and the Ansible config it writes, inside a dashed enclosure labelled
*on one laptop, not on the fleet*. No connector runs into the machines and the
fleet is unchanged from 06, because generating a file has not touched a host. That
the generator runs locally is the point of the frame, and it needs a frame of its
own or it happens between two slides where nobody sees it.

### 08 · The fleet: where lichess is now — `s08_fleet_now.py` — *machine fleet* — **explanation**

Ansible runs that config. Twenty-two cells across eight machines turn grape; sixty
cells on twenty-two machines stay red, question marks included. The connector into
the fleet returns. This is the frame that answers where lichess is today, and the
subtitle states both numbers.

### 09 · December: how a client found a service — `s09_december_discovery.py` — *icon cards* — **explanation**

Four icon-led cards with flow arrows: **OVH vrack** (network link) → **dnsmasq**
(DNS lookup) → **SRV records** (service) → **Clients** (container). The cards make
the point end to end, so the closing bar is gone; the same claim, phrased flatly,
is the closing line of slide 16 of the orchestration deck.

### 10 · December: how a service reached a host — `s10_december_placement.py` — *icon cards* — **explanation**

Four more: **`hosts.py`** (binding — which service runs on which host, written
down) → **generated config** (registry) → **systemd quadlets** (container) →
**lifecycle** (host). Closing bar: a person chose which host ran each service.

That line used to read "A human chose the host. Every time." — the same fact
delivered as an indictment, and it contradicted slide 26 of the orchestration
deck. The fact stays; the verdict goes. Slide 46 names the defect that was real.

### 11 to 14 · The playbook, paged through — `decks/golem/playbook.py`

One play, `site.yml`, with four steps, drawn four times. `playbook.STEPS` holds
each step's body and the hosts it runs against:

| step | body | hosts |
|---|---|---|
| 1 | add file | achoo, cobar, dingo |
| 2 | add line to file | manta, orbit |
| 3 | add file | radio, snafu, zulip |
| 4 | add workload | cobar, orbit, talos |

Eleven changes over nine of the thirty machines, and both numbers are derived —
`CHANGES_MADE` and `HOSTS_CHANGED` — so slide 15's count cannot drift from the
four frames that produced it. `cobar` and `orbit` appear in two steps each, which
is why a machine can carry two cells: accumulation happens inside a box as well as
across the fleet. The three bodies are Ansible's own task shapes — a file, a line
in a file, a container unit — and two of them are glyph spellings by name, with
the third reaching a `systemdService` through a quadlet. No slide says so: the
glyphs are not named until 28, and the correspondence is left for that slide to
land on.

**Each step names a host group, not the fleet.** That is the frame's argument, so
the group sizes and the particular hosts change from step to step and the groups
sit in different bands of the grid. `hosts: all` is a real Ansible pattern and
slide 05 already draws one; slide 11's subtitle is scoped to this play for that
reason.

**The kind of thing a step put there is carried by the play row, in words, not by
a per-cell mark.** A unit cell is about 35 × 23px at this geometry, which is too
small for a legible icon on a projector, so every cell these frames add is one
tone — `ANSIBLE`, because Ansible keeps it — and the play says what kind of change
it was. Cell tone still means *who keeps this unit*, and the machine border still
means *which tool did machine-level work here*, so all thirty machines carry the
Ansible border on these frames exactly as they do on 08.

What does change is how narrowly a cell is glossed. Slides 04 to 08 label a cell
*a unit a tool keeps*; 11 to 14 label it *a file, line or workload this play put
here*, because the fleet starts empty and every cell on these frames arrived
from this play; slide 15's subtitle widens the wording again to *a file, a
package, a line or a workload*. Each of those frames states its own wording
beside the grid, so no frame depends on another frame's legend.

`decks/ansible_play.draw_play` grew a `step_states` keyword for this: a step is
**current** (filled `ANSIBLE`), **taken** (`ANSIBLE` stroke on white) or **not
yet** (dotted, `INK_GHOST` outline with `INK_FAINT` text). Filled means now,
outlined means already, faint and dotted means not yet — the same three-way the
fleet boxes use, so it costs nothing to learn. An empty `step_states` draws every
step as taken, which is what the two existing callers
(`orchestration/s23_ansible_steps.py`, `machine_lifecycle/s03_the_basics.py`)
already relied on.

The fleet starts empty on all four frames, and a note in the left column says so,
because otherwise the frames would read as claiming these eleven cells are
everything on the fleet — which slides 06 and 08 have already contradicted.

### 11 · The playbook, step 1: a file — `s11_playbook_a_file.py` — *machine fleet, ordered play* — **explanation**

The whole fleet empty and dotted, the play in the left column with step 1 filled
and steps 2 to 4 not yet, and one cell on `achoo`, `cobar` and `dingo`. Subtitle:
each step in this play names a group of hosts rather than the whole fleet.

### 12 · The playbook, step 2: a line in a file — `s12_playbook_a_line.py` — *machine fleet, ordered play* — **explanation**

Step 1 outlined, step 2 filled. `manta` and `orbit` take a cell in the middle
band; the three machines from step 1 keep theirs.

### 13 · The playbook, step 3: another file — `s13_playbook_more_files.py` — *machine fleet, ordered play* — **explanation**

`radio`, `snafu` and `zulip`, along the bottom two rows. The title says *another
file* because step 3 repeats step 1's module against a different group, which is
what a play does.

### 14 · The playbook, step 4: a workload — `s14_playbook_a_workload.py` — *machine fleet, ordered play* — **explanation**

`cobar`, `orbit` and `talos`. `cobar` and `orbit` were in earlier steps, so they
now carry two cells each, and the subtitle states which two hosts repeat.

### 15 · What do we undo? — `s15_what_do_we_undo.py` — *machine fleet* — **explanation**

The same thirty boxes, each packed with a lattice of small identical marks — 36 to
50 of them per machine, ragged along the last row. Subtitle: each mark is one
thing a machine already carries, a file, a package, a line or a workload. Two
lines in the left column: the play added 11 of these marks on 9 of the 30
machines, and a playbook records what the next run will ask for rather than what
an earlier run put on a host.

**The eleven are unfindable because the module does not know which they are.**
`_prior_state_marks` runs one loop, one tone, one stroke width and one mark size
over every machine, and the number of marks on a box comes from
`FEWEST_MARKS + sum(host.name.encode()) % 15` — the host's name and nothing else.
The module imports only `CHANGES_MADE` and `HOSTS_CHANGED` from `playbook`, two
scalars that cannot carry a position, and never `STEPS`. There is no code path
that could single out an added mark, which is what makes the slide's claim true
rather than asserted. **A later edit that highlighted the eleven would make the
slide argue the opposite of what it says.**

Three things about the drawing were settled the hard way. A single filled slab per
machine was drawn first, and it fails: indistinguishability is a property of a
population, and one mark per box gives no population to hide in — coming off frame
14, where individual marks have just appeared, a featureless slab reads as detail
being taken away. Filling every box to the same number reasserts a
capacity and invites a count, which is what the earlier twelve-slot treatment was
faulted for, so the number varies and the last row is ragged. And the lattice is inset 6 units inside
`machines.cell_area` on all four sides: at zero inset the leftmost column and the
bottom row sat on the box's rounded border, which was invisible in the source and
only showed in a crop of the render.

The marks are `Tone(INK_SOFT, INK_FAINT)`, which appears nowhere else in either
deck, so it collides with no meaning already in play. On 11 to 14 the
added cells are `ANSIBLE` grape and here they are neutral like everything else,
which is deliberate: the grape on 11 to 14 is knowledge the *slides* have because
they just showed the play run, and the host does not have it. Colouring the eleven
grape here would assert a provenance the host cannot supply.

### What the sequence did to the December close-ups

It absorbed none of them. Each carries something thirty boxes cannot: 45
attributes layers to five named tools and names the job nobody took; 09 and 10 are
mechanism — dnsmasq, SRV records, `hosts.py`, quadlets — which the wide shot never
shows; 46 is the four hand-ordered steps, which is a procedure rather than a
state. Two of the four now sit inside the sequence, at 09 and 10; the other two
are in the appendix, at 45 and 46. 44's coverage bars stay for the same reason:
they measure one tool against six layers, where a fleet frame measures spread
across machines.

Only slide 46 was trimmed, and only of its golem half, which is now frame 21.

## 16 to 31 · The goals, and what golem is and how it works

16 states the goals; 17 to 21 are golem arriving on the fleet the sequence has
been drawing; 22 and 23 are the pivot; 24 to 31 are the mechanism — the pipeline,
the diff, apply and undo, the authoring contract, and the two binaries.

### The goals are one list, stated once and graded once — `decks/golem/goals.py`

`GOALS` holds five `Goal` records, each with the `statement` Dr. Dub wrote and,
where the statement packs more than one claim, the claims the scorecard grades
separately. Slide 16 draws every `statement`; slides 36 and 37 grade every entry
of `GRADED_CLAIMS`. Neither slide types a goal string, and four tests in
`test_scenes.py` hold them together: the goals slide states every goal, the
scorecard marks every graded claim exactly once across the two documents, the
scorecard's rows are the module's claims in order, and the two slides' slices
reassemble `scorecard.ROWS` exactly, so a row cannot be dropped or drawn twice by
the split. It is the same defence `decks/vocabulary.py` gives the five
orchestration job names, applied inside one deck instead of across two.

The module sits in `decks/golem/`, not in `decks/vocabulary.py`, because
`vocabulary.py` is scoped to strings both decks must agree on and only the golem
deck reads the goals.

`Goal.graded_claims` falls back to the statement with its full stop stripped, so
goals 1, 2 and 5 have no second string that could drift from the first. Goals 3
and 4 carry explicit claim tuples, because those are genuine restatements rather
than copies — *Easier to plan / be certain things will work.* is graded as
*Easier to plan* and *Being certain a change will work*. Rewording one of those
two statements alone therefore leaves its claims stale; dropping or renaming a
claim is still caught.

### 16 · What I wanted — `s16_the_goals.py` — *numbered list* — **explanation**

Five numbered statements at `GOAL_SIZE` (`HEADING_SIZE + 10`, 40pt — the same
size s38 sets its four questions), numbers in `INK_SOFT` and statements in
`INK`, the block centred between y=190 and the bottom margin. Nothing else is
drawn: no figure, no subtitle, no closing line. The longest statement, goal 3,
ends at x=1032 and the five-line block at y=804; rewording a goal means
re-measuring rather than assuming it still fits.

Two reviews pulled this size in opposite directions. At `TITLE_SIZE` (46) the
statements are the same size as the header, and "What I wanted" stops reading
as a title. At `HEADING_SIZE` (30) the slide that opens the goals section
reads too small, and no longer matches 38, which sets four statements of the
same shape at 40. 40 is where both objections are answered at once.

1. Every step undoable.
2. Static analysis and verification possible.
3. Easier to plan / be certain things will work.
4. Automated rollback on failure.
5. No YAML.

The wording is Dr. Dub's. 36 and 37 grade these five and reach a different verdict
from the one he was working from on two of them, which is the reason the
statements are drawn verbatim here rather than tidied.

### 17 · golem: emetc compiles one scroll per host — `s17_scrolls_compiled.py` — *pipeline* — **explanation**

A source tree, `emetc build`, and one manifest holding eight scrolls named for the
eight hosts golem keeps. No fleet: compiling happens on the author's machine.

### 18 · golem: each scroll goes to the machine it names — `s18_scrolls_dispatched.py` — *pipeline* — **explanation**

`golemctl fleet apply`, four scrolls, four arrows, four machines — one arrow per
host, because routing individually rather than broadcasting is the claim. Marked
*four of thirty shown*.

### 19 · The fleet: each machine assembles its own scroll — `s19_fleet_assembling.py` — *machine fleet* — **explanation**

The wide shot again, mid-apply: on golem's eight machines some cells have turned
green and the rest have not. Half-done is the state animation was wanted for, and
it is a frame instead.

### 20 · The fleet: what golem keeps — `s20_fleet_golem.py` — *machine fleet* — **explanation**

Converged. The same eight machines the generator reached, and five kinds of work
where the old stack did three — depth grew, coverage did not. Twenty-two machines
are still kept by hand, and the subtitle says so.

**This frame draws no connector from the tool column into the fleet, and that
absence is the argument.** 05 to 08 do, because a playbook and a generator act on
the fleet from outside it. One here would draw a central controller pushing, which
golem is not. Do not add it back.

The golem symbol appears here at full size, with its credit line, and on 01: those
are its two uses in the deck. The per-machine agents stay drawn marks — see *The
imported mark*.

### 21 · Moving a service: one edit, two machines — `s21_moving_a_service.py` — *before / after split* — **explanation**

`lila-gif` moves from `orbit` to `dingo`. One edit, one manifest, two scrolls, two
machines drawn large enough to count the cells in; the losing cell goes red and
dashed, the gaining cell green, and an arrow labelled with the unit runs from one
to the other. Below: **nothing orders the two, so both or neither may be running
briefly.**

That second line is load-bearing and must not be dropped. golem ships no
cross-host ordering: `golemctl fleet` spawns one task per target with no barrier
between them (`apps/golemctl/src/fleet.rs`), and no ADR or TODO proposes
otherwise. What this frame claims over slide 46 is expressibility — three
hand-sequenced edits collapsing to one — and never an orchestrated cutover.

The fleet-wide version of this frame did not work: two badges under two of
twenty-four identical boxes is a change nobody finds. The two-machine detail is
what makes it a move.

### 22 · What golem is, and is not — `s22_what_golem_is.py` — *before / after split* — **explanation**

**Not:** a replacement for bare-metal provisioning, OS installation, or the
basics of networking and security. **Is:** a replacement for the custom Python
and the new Ansible being built in December and January. A bar between them:
layer 1 stays where it is. Closing line, and the definition of *declarative* the
deck has been circling: you write the state a host should be in, and golemd works
out the steps.

### 23 · What you need, and what meets it — `s23_requirement_and_property.py` — **explanation**

Seven requirement → property rows, captioned "what you need" and "the property
that meets it":

| what you need | the property that meets it |
|---|---|
| describe the state you want | a typed program that names that state |
| take a change back | every edit records its inverse |
| drop it on any machine | a small statically linked binary (`golemd`) |
| assume nothing on the host | no interpreter and no runtime to install first |
| catch mistakes before the host | a statically typed compiler (`emetc`) |
| see a change before it lands | plan against the live host (`golemctl plan --against-host`) |
| one description for the fleet | one manifest, one scroll per host |

Two rows were false and are gone. "No interpreter, no runtime, no agent" claimed
golem puts no agent on the host, and golemd is exactly that. "Move services safely
→ reversible revisions, so drain is real" claimed a drain golem does not have:
there is no drain operation anywhere in `apps/` or `libs/`, and none is proposed
in any ADR or TODO. Do not reinstate either without the code to back it.

### 24 · The pipeline — `s24_the_pipeline.py` — *swimlane pipeline* — **explanation**

Five stages: `fleet.emet` → `emetc build` → **manifest** → `golemctl apply` →
**golemd** on the host. Subtitle: each host diffs its own scroll from one
manifest. Below the stages the manifest, quoted: `Manifest { format_version,
emet_version, scrolls: Vec<AddressedScroll> }` with `FORMAT_VERSION = 5`,
`AddressedScroll { content_id, scroll }`, and `ContentId` as a 32-byte BLAKE3
digest over postcard bytes, one per scroll and one per glyph. All verified against
`libs/scroll-format/src/manifest.rs` and `content_id.rs`.

The closing bar ("Same content id, no work.") moved to slide 25, where the ops it
refers to are actually drawn.

### 25 · Inside golemd: the diff — `s25_the_diff.py` — *before / after split* — **explanation**

Two panels. **prior** holds `&[Outcome]` — what golemd last applied, from the
journal. **desired** holds `&Scroll` — this host's scroll, selected by name from
the manifest. An arrow drops into `reconcile::plan(prior: &[Outcome], desired:
&Scroll) -> Vec<GlyphOp>`, keyed by `Glyph::key()`, and that into four op chips:
`Install`, `Remove`, `Replace`, `Noop`. Closing bar: every difference becomes one
of these four operations. Footnote: a glyph whose content id has not changed
becomes `Noop`.

**Both panels used to read `AddressedScroll { content_id, scroll }`, and that was
wrong.** `plan` does not take two scrolls — `prior` is the journalled outcome list
(`apps/golemd/src/reconcile.rs:23`). The panel headings match the parameter names
on purpose; keep them in step with the signature.

### 26 · Inside golemd: apply and undo — `s26_apply_and_undo.py` — *loop* — **explanation**

Three cards and a return arrow that closes the loop.
`Reconciler::apply(&Glyph, ContentId) -> EnactResult<Outcome>` with `Outcome { op,
cid, inverse, changed }` — apply captures the prior state as an `Inverse`, carried
on the `Outcome`. Then `Revision { id, created_at, kind, scroll_content_id,
outcomes }` with `kind: RevisionKind = Init | Reconcile`, the append-only journal
of what golem applied. Then `Reconciler::reverse(&Outcome) -> EnactResult<()>`,
which replays the `Inverse` that apply recorded. Closing bar: golem reverses only
the edits it recorded.

The gloss says what `reverse` reads, not how completely the host comes back. An
earlier wording — *restores the prior state exactly* — was contradicted by two
later slides: 35 draws a broken symlink replaced by a file whose inverse deletes
the file and never restores the link, and 36 grades *Every step undoable*
qualified because `lineInFile` does not round-trip. The limits belong on 35 and
36, where the evidence for them is drawn; this slide states the mechanism and
stops.

Both signatures are fallible and were drawn as though they were not
(`apps/golemd/src/reconciler.rs:46-47`). `scroll_content_id` is an
`Option<ContentId>`, `None` on an `Init` revision.

### 27 · One program, one scroll per host — `s27_the_scroll_tree.py` — *tree* — **explanation**

`main : List Scroll`, then a Scroll forking into a **branch** (named sub-scrolls)
or a **leaf unit** (glyphs, and an optional policy). Line under it: either glyphs
or named sub-scrolls at each level — never both. Two callouts: a leaf unit is the
failure-isolation boundary, one unit's failure never rolls back a sibling; and
workloads, quadlets and ingress are Emet libraries that compile down to the four
glyphs.

Subtitle: *Emet is an Elm-like language: a program is typed and functional, and
evaluates to a list of scrolls.* This is the only slide in the deck that says what
kind of language Emet is — the others name a file or a binary — and it is eleven
slides ahead of question 4 on the ask, which asks the room to learn Emet. See
*Resolved: what language Emet resembles*.

### 28 · The four glyphs — `s28_the_four_glyphs.py` — **reference**

Four cards, each with its Emet spelling, its Rust constructor and its
`Glyph::key()` prefix.

- `aptPackage { name }` → `Glyph::AptPackage { name }`, key `apt:<name>`
- `systemdService { unit }` → `Glyph::SystemdService { unit }`, key
  `systemd:<unit>`
- `file` / `directory` / `symlink` → one `Glyph::Filesystem { path, entry: Entry
  }`, key `file:<path>`, where `Entry = File { contents, perms } | Directory {
  perms } | Symlink { target }` and `Perms { mode: u16, owner: Option<String>,
  group: Option<String> }`
- `lineInFile { path, line }` → `Glyph::LineInFile { path, line }`, key
  `fileline:<path>:<line>`

There is no subtitle. "Each arm carries only its own fields, so illegal states
cannot be written" is the argument for the design, and this slide is the contract,
not the case for it — the speaker makes that point out loud. The gloss on the
filesystem card is now the flat "one glyph, three surface spellings".

### 29 · golemctl — on your machine — `s29_golemctl_verbs.py` — **reference**

Five verbs that address one host — `apply` (`--json`, `--reattach`), `plan`
(`--json`, `--detail`, `--against-host`), `state`, `history`, `show` — then
`golemctl fleet apply | plan | status` (`--inventory`, `--hosts`), exactly three
fleet verbs with no fleet `state`, `history` or `show`. Underneath, the apply
handshake as three chips: `POST /manifest` → `202 {"reconcile_id": <u64>}` →
`GET /reconciles/:id`, captioned *the apply handshake*.

There is no `golemctl host` subcommand: the five are top-level on `Cmd`
(`apps/golemctl/src/main.rs:15-62`). The caption under the chips used to be an
instruction — "Post the manifest, take the id, follow the stream" — which is
how-to phrasing on a reference slide; it now names the row instead.

### 30 · golemd — on the host — `s30_golemd_routes.py` — **reference**

The eight registered routes, each glossed: `POST /manifest`, `POST /plan`,
`GET /reconciles/latest`, `GET /reconciles/:id`, `GET /state`, `GET /revisions`,
`GET /revisions/:id`, `GET /status`. `against_host` and `after` are optional query
parameters, named in the glosses rather than baked into the paths, because the
paths registered in `apps/golemd/src/http.rs:54-62` are bare.

Then the conflict codes as three badges: `409 HostBusy` (a host-reading plan met
an apply in flight), `409 ReconcileInProgress` (an apply met an apply), and
**no conflict** (a plan that does not read the host never blocks). The third badge
used to read "plan still works", a clause where the other two were outcomes.

### 31 · Plan before apply — `s31_plan_against_host.py` — **explanation**

Across the top, ADR 0058's claim stated plainly: **golemd reads the host and
returns a verdict per glyph.** Below it the plan loop in four steps: `golemctl
plan --against-host` → `POST /plan?against_host=true`, where `PlanScope =
JournalOnly | JournalAndHost` and without the flag golemd reads only its journal →
`Reconciler::observe(&[GlyphOp]) -> Observations`, golemd running `dpkg-query` and
`systemctl` and reading the declared paths → `Observation = Realized | Divergent |
Absent | Unknown(Unknowable)` with `Unknowable = Sealed | Unreadable |
NotModelled`, and the verdict crossing the port while the contents stay on the
host.

`observe` is a trait method on `Reconciler`, not a free function
(`apps/golemd/src/reconciler.rs:92`). The probes are four families, not three, and
nothing is scoped to `/etc` — the filesystem probes read whatever absolute path
each glyph declares (`apps/golemd/src/reconcilers.rs:508-542`). The
contents-stay-on-the-host claim holds: `Observation` and `Unknowable` are not
`Serialize` at all, and `PlannedOp` carries only the four-valued tag
(`apps/golemd/src/plan_report.rs:61-76`).

The routes, verbs and flags on 29, 30 and 31 are quoted from shipped code and go
stale when it moves. Check them against
`sites/website/src/content/docs/reference/cli.mdx` and
`docs/adr/0058-the-plan-reads-the-host-and-only-a-verdict-crosses-the-port.md`.

## 32 to 38 · The caveats, the grading, and the ask

Seven slides that state what golem does not do and what is wanted from the room.
33, 34 and 35 draw their limits from the code they describe, and the two grading
slides carry the evidence for each verdict on the row with it.

### 32 · Outcome-based development — `s32_outcome_based_development.py` — *pipeline* — **explanation**

Subtitle: how golem itself was built. A four-box chain across the top — **Prompts**
→ **An LLM** (writes the software) → **The software** (golem itself) → **The
outcome** — and beneath it a review row labelled *What a person looks at*, with a
slot under the last two boxes only.

The slot under **The outcome** is solid, holds `icons.person`, and is reached by a
solid arrow; it is captioned *review of the outcome*. The slot under **The
software** is the same box drawn empty, dotted and `INK_GHOST`, reached by a
dotted arrow, captioned *no review of the code*. No sentence on the slide states
that the code goes unread — the empty box beside the full one is the claim, which
is the precedent `machine_lifecycle/s10_what_changes.py` set: an absent artifact
is drawn absent rather than described as missing. Filling that slot with a caption
would remove the argument rather than restate it.

`icons.person` was added for this slide. The catalogue had no human figure of any
kind, and the filled slot needs a subject for the empty slot beside it to read as
*nobody* rather than as *nothing drawn yet*.

### 33 · Where golem is today — `s33_current_status.py` — *recap strip, panel* — **reference**

One neutral strip across the content width — *Outcome-based, and no review of
the code* — then a panel headed **The site that serves golem's documentation**,
with a `golem.yyc.dev` mono chip, one line of mechanism (an Emet program
describes the site; golemd reconciles it on the host), four Emet literals quoted
verbatim, and two correcting lines:

```
scroll { name = "dull-01" }
aptPackage { name = "podman" }
file { path = "/etc/containers/systemd/golem-docs.container" }
systemdService { unit = "golem-docs.service" }
```

The two correcting lines are load-bearing. *The program lives in dulliac, a
separate repository and golem's first outside consumer*, and *golem's own
repository publishes the container image; it does not deploy it*. golem's release
workflow builds and pushes `ghcr.io/dull-ca/golem-docs`; the Emet program that
puts it on a host is `dulliac/fleet/main.emet` and `dulliac/fleet/sites/`. The
panel heading names the site rather than the host, because what golem keeps is the
container unit, the service and the route on `dull-01`, not the whole machine.
`examples/website/website.emet` in golem's own repository is a different site — it
provisions `remora` for the self-hosted-CI loop — and must not be cited here.

The strip is a recap and is sized as one. Both of its facts are slide 32's whole
subject and slide 32's whole argument, the second of them drawn there as an empty
box rather than written out, so drawing them here at the same weight as the panel
would tell 32 a second time. The panel is the only thing this slide says that 32
did not, and it gets the canvas. The wording matches 32's caption — *no review of
the code* — so the two slides state one claim at one strength.

### 34 · The Emet language today, and its limits — `s34_longer_term_goals.py` — *two panels* — **explanation**

Subtitle: I want better templating, and configuration shapes as first-class
types. The left panel, solid green, is **What a program can say today**: the
four glyph spellings as mono chips — `aptPackage`, `systemdService`, `file` /
`directory` / `symlink`, `lineInFile` — under the line *Routes, port exposure
and quadlet units are already Emet types.* The right panel, dotted, is **What
the language does not do yet**, three dotted boxes:

- **A multi-line string literal** — a config file is written as one-line strings
  joined with a newline. A raw newline before a closing quote is an "unterminated
  string literal" (`apps/emet/src/lexer.rs:438-443`), which is why every config
  file in `lib/` is a `String.join "\n"`.
- **The shape of a rendered config file** — a Traefik config or an nftables rule
  is file contents, and nothing type-checks the rendered text. Every glyph field
  unifies with `String` (`apps/emet/src/infer.rs:1409-1445`), and `emet` links no
  YAML, nftables or unit-file parser.
- **File ownership** — the wire model carries an owner and a group; the language
  does not set them.

**Neither panel asks for a fifth glyph kind.** An earlier draft of the right panel
drew networking primitives as glyphs of their own, which proposes exactly the
thing the architecture rules out, and was false as an absence besides:
`Routing.Route`, `Traefik.Ingress` and `Quadlet.Expose` are Emet types today. The
three gaps as drawn all sit inside the four-glyph model.

The right panel's heading reads *What the language does not do yet* rather than
*What it cannot say yet*, because a program can express multi-line contents —
awkwardly, through `String.join` — and the stronger heading would have been false
of its own first box.

The title used to read *Longer-term goals*, two slides before **Grading the
goals: 1 to 3** and **Grading the goals: 4 and 5**, which grade slide 16's five
statements. A room that had just seen a slide titled *goals* would read *the
goals* as this slide's three boxes. The module filename and the slug still say
`longer-term-goals`. The slug names the built file, so changing it renames
`34-longer-term-goals.excalidraw` — a separate change from the title the room
reads.

### 35 · When things fail — `s35_adversarial_conditions.py` — *three panels* — **explanation**

Three columns of reconciler behaviour, each headed by the result variant it is:
**Refused** (`Fatal`, never retried), **Retried** (`Retryable`, five attempts by
default, then the unit rolls back), and **Absorbed** (`Ok`, no failure reported,
and no record that puts the prior state back). The third column is in `GAP` red
and is why the slide exists; a slide that listed only what golem handles well
would not be a caveat.

Refused: a file already where a directory is declared, a file already where a
symlink is declared, a symlink already pointing somewhere else, a prior file that
is not UTF-8. Those are the four `Fatal`s an Emet program can actually reach — the
unresolvable-owner arm is unreachable while the evaluator hardcodes owner and
group to `None`, so it is not drawn. Retried: a read-only filesystem or permission
denied, a directory where a file is expected, the dpkg lock held by another
process, a unit latched failed with its start limit burnt. Absorbed: a broken
symlink at a declared file path is replaced and reverse deletes the file rather
than restoring the link; ACLs, extended attributes and SELinux labels are neither
observed nor restored.

Every case carries its `apps/golemd/src/reconcilers.rs` line range as a comment in
the module, so a later edit has to argue with the citation rather than with the
prose. The subtitle states how the reconcilers are exercised — tempfiles and a
fake command runner, with the end-to-end install-to-decommission run against a
real Debian box deferred — and deliberately does not say "untested against a
failing host", which the tempfile tests would contradict.

At 138 body words this is the deck's wordiest slide. Every word of it is a four-
to eight-word failure case in a three-column enumeration; if it has to lose
weight, the honest cut is one *Refused* case and one *Retried* case, never the
*Absorbed* column.

### 36 · Grading the goals: 1 to 3 — `s36_grading_goals_one_to_three.py` — *scorecard rows* — **explanation**

### 37 · Grading the goals: 4 and 5 — `s37_grading_goals_four_and_five.py` — *scorecard rows* — **explanation**

Seven graded claims over two slides. They do not fit one canvas at the type floor,
so the grading splits, and the cut is on a goal boundary rather than down the
middle of the seven rows: a goal that states two claims keeps both on one slide.
Both slides start their first row at the same y, so flipping between them moves
nothing but the rows.

A row is a goal number, a mark, the verdict word, the claim at `HEADING_SIZE` and
the evidence at `BODY_SIZE`. Where one goal contributes two rows to a slide, the
numbers read `3a.` / `3b.` and `4a.` / `4b.`, computed from the rows the slide
actually draws — two rows both labelled `3.` are correct but read as a copy-paste
error to anyone who has not seen slide 16.

| # | claim | mark | evidence |
|---|---|---|---|
| 1 | Every step undoable | qualified | All four glyph kinds reverse. `lineInFile` does not round-trip, and a failed reverse is logged rather than retried. |
| 2 | Static analysis and verification possible | qualified | The type checker infers types and checks case exhaustiveness. Analysis across a fleet is one rule, which misses the conflict in the repo's own smoke fixture. |
| 3a | Easier to plan | achieved | Planning against a live host returns a verdict for every glyph. |
| 3b | Being certain a change will work | qualified | The plan above is a real check against a live host. The compiler checks nothing against a host, and detects neither cross-glyph conflicts nor dependency order. |
| 4a | Automated rollback on one host | achieved | Rollback is the default when retries are exhausted. It reverses the failing leaf unit, the failure-isolation boundary. A scroll can opt out; the one serving golem's documentation does. |
| 4b | Automated rollback across the fleet | not achieved | A fleet apply spawns one task per target, with no barrier between hosts and no fleet-wide reverse. |
| 5 | No YAML | achieved | The authoring language is Emet; the fleet inventory is TOML. |

Rows 1 and 4a carry a monospace chip at the row's right margin — `lineInFile` and
`policy = keep` — the same idiom slide 47 uses for `--check` and slide 23 for
`golemctl plan --against-host`. The sentence still names what the chip names, so
each row reads cold; the chip carries the exact spelling.

**Row 1 and row 4a both overturn what the deck's author believed**, and the
evidence for the reversal is in *Corrections against the code*. `Being certain a
change will work` reads *qualified* rather than *not achieved* because
`plan --against-host` does return a real per-glyph verdict; there is no fourth
mark state for "partly".

### The three mark states

`icons.achieved`, `icons.qualified` and `icons.not_achieved`, at
`GOAL_MARK_ASPECT = 1.0`, each a solid body in the state's colour with a white
glyph inside, and each row's card filled with the state's pale tone:

- **achieved** — a filled circle with a white check
- **qualified** — a filled diamond with a white exclamation
- **not achieved** — a filled square, sharp-cornered, with a white minus bar

The outline and the interior glyph each carry the state on their own, so **colour
is redundant rather than load-bearing**: green and red read as the same grey at
the back of a room, and the shapes still separate in a greyscale crop. Two states were not
enough: three of the seven rows are qualified, and grading any of them achieved or
not achieved would have been false in one direction or the other.

`not_achieved` is not `not_applicable`, the red X on slide 03. That mark means the
claim does not apply; this one means the claim was graded and failed. They are
never interchangeable.

### 38 · Looking for feedback — `s38_looking_for_feedback.py` — *numbered list* — **explanation**

Four numbered questions at 40pt, in Dr. Dub's words:

1. Worth pursuing?
2. Ideas you'd like to see added?
3. Can I start using it to manage some boxes, like the irwin stack?
4. Will you help, and learn Emet?

No figure, no legend, no subtitle and no closing line: this is the slide that
stays on screen while the room talks, so nothing on it may need explaining.

40pt is content-limited rather than chosen. Question 3 is the long one: at 40pt it
measures 1303 units and ends at x=1463, inside the 1536 margin; at `TITLE_SIZE`
(46) it measures 1499 and would end at 1659, well outside it, and at 44 it is
already over. Rewording question 3 means re-measuring rather than assuming the
size still holds.

## 39 to 47 · The appendix

Nine slides kept in the deck for the speaker to cut live rather than removed from
it. They hold the ladder, the six-layer stack, what orchestration means, what
buying it would cover, Ansible's coverage, December's owners, the move that took
four hand-ordered steps, and the five problems — in the relative order they ran in
before the reorder.

### 39 · Where lichess sits — `s39_where_lichess_sits.py` — *timeline* — **explanation**

The six columns of 03 as rungs on an axis, from `decks/golem/lichess_ladder.py`,
with a badge over the first rung: **lichess is here**.

Below the axis, a split whose two headings are the claim and whose bodies are the
columns each covers: **You name the machine** — bare metal with configuration
management, and Docker on one host; **The platform picks the machine** — Swarm,
Nomad, Kubernetes, managed Kubernetes. The headings used to read "Left of the
middle" and "Right of the middle", which named a position on the canvas rather
than a thing, and left the two bodies to carry the actual point.

### 40 · Where lichess sits, with Portainer — `s40_where_lichess_sits_with_portainer.py` — *timeline* — **explanation**

The same ladder at identical geometry, plus a bar spanning Docker through
Kubernetes: **Portainer — a web UI that manages these platforms**, in the layer-6
tone because that is the work it does. Below it, thirty machine marks with one
filled, and the fact stated flat: thirty machines, under Ansible, hand
configuration and custom Python, and exactly one of them runs Portainer. Thirty is
the host count in `inventory/hosts.yaml`; which box runs Portainer is not recorded
there, so the slide states the count and never names the host.

The two slides were one, which put Portainer at the scale of a rung. It is a web
UI on one box. Drawing the count is what corrects it, and the thirty marks are the
same thirty machines the fleet sequence draws.

### 41 · What lichess runs — `s41_lichess_stack.py` — *layered stack* — **reference**

The shared figure, from `decks/golem/lichess_stack.py`. Five bands drawn top to
bottom as 5, 4, 3, 2, 1, plus layer 6 as a tall column to the right spanning
bands 2–5. Layer 1 is the only band drawn full width — it runs under the column
too.

1. **Core OS, network, security** — Debian, kernel, sshd, nftables, TLS
2. **Application hosting** — podman, systemd, storage, registry access
3. **Connective infrastructure** — DNS, SRV records, proxies, load balancers
4. **Tools, dependencies, runtimes** — JVM, node, native libs, base images
5. **The applications** — lila, lila-ws, lila-search, mongodb, redis
6. **Lifecycle / schedule / scaling** — the right-hand column, subdivided into
   the five parts of slide 42

Those band details are trimmed to one line at 24pt. The fuller enumerations they
came from are: layer 1 also users and the private network; layer 2 also volumes;
layer 3 also service discovery, reverse proxies and the private network fabric;
layer 4 also client libraries; layer 5 also "the rest". They are the speaker's
sentences now, not the slide's.

`lichess_stack.draw()` takes per-layer and per-part `Tone`s and tags but **no
geometry**. Slide 43 recolours this figure rather than redrawing it, and
`decks/golem/fleet.py` reuses its `DESCRIPTIVE_LAYER_TONES` for every machine.

The only prose is one line under the figure, explaining the shape a reader is
looking at: layer 6 runs across layers 2 to 5, so it is drawn beside them.

### 42 · What orchestration means — `s42_orchestration.py` — *radial hub* — **reference**

A hub labelled **Orchestration** — *layer 6 of the stack* — with five satellites,
the five parts named once in `decks/vocabulary.py` and reused verbatim here, in
the column on 41 and 43, and throughout the orchestration deck:

- **Placement** — choosing which node runs a workload
- **Lifecycle** — start, stop, restart, drain, rolling update, rollback
- **Health and reconciliation** — watch actual state, detect drift or failure,
  reschedule
- **Supporting plumbing** — networking, service discovery, load balancers,
  storage, secrets
- **Scaling** — replica counts moved by policy or load

The hub is the one box on this canvas that must name the thing, and it once read
`Layer 6 / one word` — a fragment of the slide's own subtitle, unreadable alone.
Placement's gloss was "the scheduler — the only part that answers which node",
which asserted a property before the noun had been defined; the definition is now
the gloss, and the only-part claim is a closing line on slide 11 of the
orchestration deck, where it belongs.

Closing line: each of these is done by a platform, a script, or a person. A person
doing one is an answer, not a failure.

### 43 · If we bought orchestration — `s43_bought_orchestration.py` — *layered stack* — **explanation**

The slide-41 figure recoloured, at identical geometry. Nomad or Kubernetes covers
layers 2, 3 and 6 and every orchestration part inside 6; the OCI image supplies 4
and 5; layer 1 is ours. A dashed callout: renting managed Kubernetes covers layer
1 too — and costs more. Subtitle: a platform provides all five parts of layer 6
together. The legend reads *provided by the platform* / *provided by the image* /
*ours to operate* — an image does not "carry" anything.

### 44 · What Ansible managed — `s44_ansible.py` — *coverage bars* — **reference**

Six tracks, one per layer. Ansible managed 1, 2 and 4 outright; layer 3 was mostly
by hand; layers 5 and 6 were by hand, and layer 6's track is tagged *by hand — all
five parts*. Legend: *managed by Ansible* / *done by hand*.

The tags used to read "Ansible reached it", "still ours" and "a human decided and
did it" — an invented idiom that made a playbook an agent with reach, and made a
person deciding sound like a fault. The bars and the legend now carry the whole
slide; there is no closing line.

Drawn as bars rather than as a third recolouring of the stack. The absence is the
point of the slide, and an empty track shows it more directly than a different
fill on the same rectangle — and it stops slides 41 through 45 reading as one
figure four times.

### 45 · December: who owned what — `s45_december_owners.py` — *card rhythm* — **reference**

Five cards in a 3 / 2 / 1 rhythm: Ansible → layers 1 and 2; custom Python → layer
3; quadlets → layers 4 and 5; custom Python + Ansible → placement, plumbing and
scaling; systemd → lifecycle. Then, full width and red: **Nobody** → nothing
watched for drift or failure. Closing line: layer 6 had no single owner.

### 46 · December: moving a service — `s46_december_moving_a_service.py` — *numbered steps* — **explanation**

The four hand-ordered steps a move took: **1** edit the definition, marking the
service disabled → **2** apply to host A, where it stops and uninstalls → **3**
edit again, removing it from A and adding it to B → **4** apply, and it installs
on B. Note: out of order, it runs on both hosts or on neither.

**The golem half of this slide is now slide 21**, where the move is drawn on the
fleet. It used to sit here as a green bar and a limitation note, which made this
slide promise something two frames later would show; the promise and the drawing
are now in one place. The limitation travelled with it and is still load-bearing.

This slide replaced *December: what it could not do*, which listed Drain, Move a
service and Roll back as three missing operations. Two of those survive elsewhere
— "No undo" is problem 2 on slide 47 — and the third was the interesting one, but
its old gloss ("placement changed only by editing the table") named a person's
decision as the fault instead of naming the missing mechanism.

### 47 · Where it broke — `s47_where_it_broke.py` — **explanation**

Five numbered problems, each a heading and one line:

1. **Ansible is imperative mutation** — each task has to be written to be
   idempotent; nothing checks that it is
2. **No undo** — every rollback written by hand, as another play
3. **No static analysis** — the dry run cannot evaluate every task, so errors
   appear on a live host. `--check` is a monospace chip on the row rather than a
   word in the sentence
4. **No way to test against a known-good host** — no way to see what a change
   would do before running it
5. **Tied to the newest podman and Debian trixie** — every host had to be on the
   newest release

No subtitle and no closing line: five numbered rows announce that they are five
problems, and "the cost of writing changes as steps" was a fragment standing in
for the argument the speaker makes out loud.

---

# The orchestration deck

## The argument

A cluster is not a new kind of thing; it is a pile of ordinary things with one
decision added. The deck builds that pile from the bottom, one idea per slide,
each carrying its own mark so the vocabulary is learned by seeing it rather than
by being told.

**01 to 05 are one machine.** A process on a host shares one filesystem, one
network and one set of library versions with every other process; a container is
still a process on that host, with an image, namespaces and cgroups added; an
image is read-only layers plus a config, named by its digest; the registry is the
only thing a host has to reach; and the container runtime does all of this on one
machine and none of it across machines.

**06 is the hinge.** Many hosts means a control plane, workers and a
desired-state store — you name what should run, and the control plane chooses
where.

**07 names the five jobs**, using the same five names as slide 42 of the golem
deck, because they are the same five jobs. **08, 09 and 10 answer them three
times**, at 07's geometry, so 08 reads as 07 answered and 09 and 10 read as the
same slide again at a later date: what lichess uses today, what it planned in
December, and what it would use with golem. The answer chips take the fleet
frames' notation — red and dashed for by hand, solid for a tool — so both decks
say "by hand" the same way, and 08's count sits in its subtitle because a viewer
would otherwise have to count it.

Today, systemd keeps lifecycle, health is a mix of by hand, monitoring and
systemd, and placement, plumbing and scaling are done by hand. December's plan
puts Ansible on placement and answers plumbing with Ansible, dnsmasq and SRV
records. golem takes over the enacting half of placement and joins systemd on
lifecycle. Health, plumbing and scaling are identical on 09 and 10, which is the
sequence's other claim: most of the list does not move.

**All three carry a configuration-management row, and Ansible holds it in every
one.** It is not one of the five, so it is drawn outside them — below a rule,
unnumbered, unboxed, its label in the recessive tone — rather than as a sixth
box that would quietly make orchestration six things. The thing that never
changes hands is the thing nobody is proposing to replace.

11 through 18 then take four of the jobs one at a time: placement (11, 12),
lifecycle (13), health and reconciliation (14), and supporting plumbing spread
across connectivity (15, 16), scaling (17) and storage and secrets (18).

**11 is the slide the deck exists for.** Placement is the only part that chooses
a node, and it is drawn as an act: an unplaced workload, the candidate nodes, and
the binding that settled it. The definition comes first — a binding is the record
that this workload runs on that node — and the only-part claim follows it as the
closing line.

**19 to 26 land it.** 19 is one matrix across Docker, Swarm, Nomad and
Kubernetes — who provides which piece. Then a sequence that starts on the whole
stack and ends on golem's band of it.

**20 and 21 are one figure twice.** 20 draws the seven bands and cuts them where
a provider stops selling and you start configuring; 21 keeps the geometry, drops
the colour out of the bands, and lets six answers carry it — OVH, by hand,
Ansible, Kubernetes, Nomad, Portainer. Several bars cover the same band, two
bands carry no bar at all, and no bar runs the height of the stack. Portainer is
a dashed enclosure over Kubernetes and Nomad rather than a bar, because it is a
web UI over them and not a band of its own.

**22 and 23 are how Ansible works**, and they are two slides because coverage and
mechanism are two claims. 22 is the thirty real hosts with one box reaching every
one of them in a single run. 23 is one play: four quoted tasks running top to
bottom into a host, the host left as whatever they left behind, and the rule that
each task has to be *written* to be safe to run twice — the model does not check.

**24 and 25 are what golem is**, for a room that has not met it. 24 draws the
promise: each machine holds the state it should be in, acts on itself, and the
arrow between two machines is crossed out. Nothing orders one against the next,
which is why a service moving between hosts can be on both or on neither for a
moment — the same limitation slide 21 of the golem deck states, arriving here as
a consequence of the model rather than a caveat bolted on. 25 is the timeline
that keeps 24 honest: a submit, then drift that nothing responds to, then a
host-reading plan that reports and changes nothing.

**26 closes on 20's stack** with golem's bands marked and, inside the
orchestration column, placement and scaling drawn in the by-hand notation. It and
slide 10 have to be reconcilable by anyone who sees both, which is why 10 keeps
the by-hand mark on placement rather than handing the row to golem.

## The twenty-six slides

| # | Title | Module | Form | Mode |
|---|---|---|---|---|
| 01 | A process on a host | `s01_a_process_on_a_host.py` | split / cards | explanation |
| 02 | What a container adds | `s02_what_a_container_adds.py` | icon cards | explanation |
| 03 | The image | `s03_the_image.py` | hub / stack | explanation |
| 04 | Registry, pull, run | `s04_registry_pull_run.py` | icon cards, flow | explanation |
| 05 | One host, many containers | `s05_one_host_many_containers.py` | host with workloads | explanation |
| 06 | Many hosts: the cluster | `s06_many_hosts_the_cluster.py` | cluster map | explanation |
| 07 | The five jobs | `s07_the_five_jobs.py` | numbered stack | reference |
| 08 | What lichess uses for each | `s08_what_lichess_uses.py` | numbered stack, chips | reference |
| 09 | What lichess planned in December | `s09_the_december_plan.py` | numbered stack, chips | reference |
| 10 | What lichess would use with golem | `s10_with_golem.py` | numbered stack, chips | reference |
| 11 | Placement: the binding | `s11_placement_the_binding.py` | binding mark | explanation |
| 12 | What the scheduler weighs | `s12_placement_what_it_weighs.py` | hub / cards | explanation |
| 13 | Lifecycle | `s13_lifecycle.py` | state machine | reference |
| 14 | Health and reconciliation | `s14_health_and_reconciliation.py` | loop | explanation |
| 15 | Connectivity: addressing | `s15_connectivity_addressing.py` | before / after split | explanation |
| 16 | Connectivity: the service | `s16_connectivity_the_service.py` | icon cards, flow | explanation |
| 17 | Scaling | `s17_scaling.py` | replica set | explanation |
| 18 | Storage and secrets | `s18_storage_and_secrets.py` | split | explanation |
| 19 | Who provides which piece | `s19_who_provides_which_piece.py` | matrix | reference |
| 20 | The stack, and where you take over | `s20_the_stack.py` | layered stack | explanation |
| 21 | Which product answers which part | `s21_which_product.py` | layered stack, span bars | explanation |
| 22 | Ansible: one controller, every machine | `s22_ansible_pushes.py` | machine fleet | explanation |
| 23 | Ansible: a play is an ordered list of steps | `s23_ansible_steps.py` | numbered steps | explanation |
| 24 | A promise is about your own state | `s24_a_promise.py` | machines, self-loops | explanation |
| 25 | golem reconciles when it is told to | `s25_on_demand.py` | timeline | explanation |
| 26 | Where golem sits | `s26_where_golem_sits.py` | layered stack | explanation |

**Each of 01 to 06 and 15 opens on a definition, because each introduces a term.**
A process is a running program sharing one machine with every other program. A
container is a process on the host, given three things it did not have. An image
is read-only filesystem layers plus a config, named by the digest of its contents.
A registry is a server that stores images and serves them by digest. A container
runtime is the program that runs containers on one host. A cluster is many hosts,
a store of the state you want, and a control plane. A service is one stable name
for a changing set of instances. Those seven lines are the deck's whole vocabulary
teaching, and each replaced an evocation — "Layered, content-addressed, and never
edited in place", "Desired against actual, forever", "Two ways to wire it".

Slide 19's shape, and why it is worth drawing: Docker on one host leaves almost
everything to you; Swarm and Kubernetes provide almost all of it; Nomad provides
most of it but leaves supporting plumbing and secrets to Consul and Vault. That
asymmetry is the information.

### The three states — `decks/orchestration/job_answers.py`

08, 09 and 10 are one figure with three sets of answers on it. `draw()` takes the
answers and the tool that holds configuration management, and **no geometry**:
the five boxes stand where slide 07 put them, so a viewer flipping 07 → 08 → 09 →
10 sees only the right-hand column change.

An answer is chips, and the relation between them is the notation:

- **Chips separated by a gap are a mix.** Health carries three of them — by hand,
  monitoring, systemd — and means all three.
- **Chips joined by an arrow are a decision and its enactment**, and each carries
  the half it does. Placement on 09 reads `by hand / chooses the host` →
  `Ansible / installs it there`, and on 10 the same with golem. The two labels
  are why the pair cannot be misread as a mix, and why the row does not need the
  legend or the subtitle to say what it means.

**Neither Ansible nor golem chooses a host.** golem is deliberately not a
scheduler, 26 draws placement in the by-hand notation, and if 10 handed placement
to golem the deck would contradict itself sixteen slides later. So the by-hand
mark stays on placement in all three states and only the enactor changes, which
is the whole content of the row.

**Lifecycle on 10 is golem *and* systemd**, drawn as the same pair:
`golem / enables and starts it` → `systemd / keeps it running`. `apply_systemd`
runs `systemctl daemon-reload` and `systemctl enable --now`
(`apps/golemd/src/reconcilers.rs`), so golem sets a unit's state through systemd
and systemd is what runs it. Replacing systemd on that row would be false.

**golem gets no chip on health**, on 10 or anywhere. Drift is reported by an
opt-in host-reading plan and never corrected — see *The claims 25 and 26 must not
make* — so a green chip on the reconciliation row would be the self-healing claim
in another form.

The configuration-management row is drawn outside the five: below a rule, without
a number, without a box, its label in `INK_SOFT` and its chip in the answer
column so it stays comparable. Four signals, no caption. The legend moved into
the header to make room for it, which also puts the key beside the title on all
three.

### The shared stack figure — `decks/orchestration/stack.py`

Seven bands, bottom to top: facility and power, bare metal, network, operating
system, application hosting, tools and runtimes, the applications. Orchestration
is a column beside the top three rather than a band above them, for the reason
the golem deck gives its own layer 6 — it acts across those bands. Bands 1 to 4
run full width; the column stands beside 5, 6 and 7.

`draw()` takes tones, tags and a per-part tone, and **no geometry**. 20, 21 and 26
are pixel-identical, so flipping between them changes colour and nothing else.
The right-hand gutter is where a slide says who answers what: `gutter_bar` spans
the bands one answer covers, and three lanes let two answers to the same band sit
side by side. `enclose` is the dashed box for something that sits over other
answers without being one of them.

The buy-versus-configure tones on 20 are `THEIRS` and `YOURS`, the same pair the
golem deck's two matrices use, because it is the same distinction.

### 25 · golem reconciles when it is told to

The slide that stops the promise-theory framing from overclaiming, and every line
on it was checked against the code before it was drawn.

golemd has **no timer, no watcher and no loop**. `main` binds a listener and
serves; the only long-lived call is `axum::serve`, and `packaging/golemd.service`
has no timer sibling. A reconcile happens if and only if something POSTs
`/manifest`. Startup runs `recover()`, which settles an interrupted attempt and
does not re-apply the last scroll — a daemon restarted on a drifted host does
nothing.

The apply diff **never reads the host**. `plan(prior: &[Outcome], desired:
&Scroll)` folds the write-ahead log, so a glyph whose content id has not moved is
a `Noop` that enacts nothing. Re-submitting the same manifest after someone
`apt remove`s a package fixes nothing.

Drift is **reported, never corrected**. `golemctl plan --against-host` is the
only thing that looks at the host, it is read-only, and ADR 0058 states that no
`Observation` reaches `run_reconcile`.

So the slide says *on demand*, and does not say *eventually consistent*. There is
no anti-entropy mechanism, and the peer gossip that would spread a manifest host
to host is ADR 0039's design with **no code behind it** — golemd has no HTTP
client outside its dev-dependencies. See *The claims 25 and 26 must not make*.

### 26 · Where golem sits

golem takes bands 5, 6 and 7. Band 4, the operating system, stays Ansible's — the
golem deck's slide 22 says golem replaces neither OS installation nor the basics
of networking and security, and bands 1 to 3 are bought.

Inside the orchestration column, **placement and scaling are drawn in the by-hand
notation** and the other three in golem's green. Nothing in golem chooses a node,
and nothing in it carries a replica count: two instances are two glyphs someone
wrote down. The closing line — placement and scaling stay decisions a person
makes, written down and versioned — is the deck's position on manual placement,
and slides 10 and 46 of the golem deck are written to agree with it. If the two
ever disagree again, this is the one that is right.

**"Answers" is not a verb for a tool.** A platform provides, a person decides, a
runtime does. Legends across both decks read *provided by the platform* / *you
provide it*, and the same discipline retires "a playbook reached it" and "the
image carries it".

---

## The claims 25 and 26 must not make

Promise theory and eventual consistency are the natural framing for what golem
does, and four of the sentences they suggest are false. Each was checked against
the code, and no slide may reinstate one without the code to back it.

**"golemd continuously converges the host."** There is no timer, no interval and
no background task anywhere in `apps/golemd/`. One bounded pass per submitted
manifest.

**"golem self-heals" or "corrects drift automatically."** Drift on a glyph whose
content id has not moved becomes a `Noop` that enacts nothing
(`apps/golemd/src/foreman.rs`, and the test
`reapplying_same_scroll_is_noop_but_still_journals`). ADR 0058: nothing in the
host-reading plan starts a unit or clears a latch.

**"Eventually consistent."** There is no anti-entropy mechanism. ADR 0039 is a
*proposal* — it would flood raw manifest bytes to a static peer set, it disclaims
anti-entropy in its own consequences, and it has no code: golemd carries no HTTP
client outside `[dev-dependencies]`, no `--peer` flag and no `[fleet]` config.

**"A central controller works out what each host should do."** `golemctl` ships
manifest bytes. Each daemon selects its own scroll by host name and computes its
own diff against its own journal, and ADR 0058 rejects client-side diffing
outright.

One qualification on the promise reading itself, which no slide currently draws:
a manifest that omits a host resolves to the *empty* scroll, which is a removal
order rather than silence. `golemctl fleet` works around it by skipping hosts the
manifest does not name, and ADR 0039 lists fixing it in golemd as a precondition
for gossip.

## Corrections against the code

Earlier drafts got these wrong. All are drawn correctly now; they are recorded so
nobody reintroduces the error. Each was found by reading the definition, not by
paraphrasing the previous slide — which is how several of them survived as long as
they did.

**`reconcile::plan` does not take two scrolls.** `plan(prior: &[Outcome], desired:
&Scroll) -> Vec<GlyphOp>` (`apps/golemd/src/reconcile.rs:23`). Slide 25 drew both
panels as `AddressedScroll { content_id, scroll }`; `prior` is the journalled
outcome list.

**`apply` and `reverse` are fallible.** `apply(&Glyph, ContentId) ->
EnactResult<Outcome>` and `reverse(&Outcome) -> EnactResult<()>`
(`apps/golemd/src/reconciler.rs:46-47`). Slide 26 drew both as infallible.

**There is no drain, and no cross-host ordering.** No drain operation exists in
`apps/` or `libs/`. `golemctl fleet` spawns one task per target and joins them
afterwards, with no barrier, dependency edge or concurrency limit
(`apps/golemctl/src/fleet.rs`); a failure on one host neither stops nor rolls back
another. Within one host, `plan` orders installs and replaces first and removes
last (`reconcile.rs:20-22`). Slide 23 claimed "so drain is real"; slide 21 must
keep saying that nothing orders host A before host B.

**golemd puts no agent-free binary on the host — golemd *is* the agent.** Slide
23's "no interpreter, no runtime, no agent" was self-contradicting.

**`observe` is a trait method**, `Reconciler::observe(&[GlyphOp]) ->
Observations` (`apps/golemd/src/reconciler.rs:92`), and the probes are apt via
`dpkg-query`, systemd via `systemctl`, and direct filesystem syscalls at whatever
absolute path each glyph declares. Nothing is scoped to `/etc`.

**`/plan` and `/reconciles/:id` are registered bare**; `against_host` and `after`
are optional query parameters (`apps/golemd/src/http.rs:54-62`).

**`Init` and `Reconcile` are variants of `RevisionKind`, not of `Revision`.**
`Revision` is a struct — `{ id, created_at, kind, scroll_content_id, outcomes }`
— and `kind: RevisionKind` is where the two variants live
(`apps/golemd/src/journal.rs`).

**`Inverse` is a field of `Outcome`, not an argument.**
`Reconciler::apply(&Glyph, ContentId) -> Outcome` returns `Outcome { op, cid,
inverse, changed }`, and `Reconciler::reverse(&Outcome)` takes the whole
`Outcome` and reads `outcome.inverse` from it (`apps/golemd/src/reconcilers.rs`).

**Rollback on failure is automatic, and it is the default.** An earlier session
recorded that `recover()` settles an interrupted attempt without re-applying, and
concluded that rollback was probably operator-triggered. Both halves were wrong.
`config.rs:100` sets `on_exhaust: OnExhaustConfig::Rollback`; when retries are
exhausted `foreman.rs:1005-1009` calls `rollback_unit`, which reverses that unit's
write-ahead-log steps last-in-first-out (`:1776-1789`); and `recover()`
(`:1794-1819`) does re-apply — `redrive_intended` re-runs every `Intended` step
without a terminal outcome, and the whole interrupted attempt is then rolled back
and marked `RolledBack`. There is no rollback, revert or undo verb in `golemctl`
at all (`apps/golemctl/src/main.rs:15-112`); rolling back to a previous revision
means re-applying the previous manifest as an ordinary forward apply. Slide 37
grades this **achieved**. Its scope is the failing leaf unit, which is the designed
failure-isolation boundary rather than a shortfall, and a scroll can opt out with
`policy = keep`.

**"Every step undoable" is qualified, not achieved.** The mechanism is complete —
`journal.rs:93-127` defines nine `Inverse` variants and `reconcilers.rs:722-749`
dispatches all nine, with no `todo!()` and no catch-all arm — but three holes are
real. `lineInFile` does not round-trip: `append_line` creates the file when it is
absent while `RemoveLineInFile` rewrites it empty, leaving an empty file where
none existed, and the trailing newline it adds is never removed. Parent
directories created by `write_file_atomic`, `append_line` and the symlink arm are
not recorded in the inverse, which carries only the leaf path. And a failed
reverse is logged as `"rollback step failed"` and the step is still marked
`Reversed` (`foreman.rs:1747-1749`), so reverse is best-effort. Slide 36 grades
this **qualified**.

## Resolved: the shared figure's geometry

It is now one geometry, and it is not a parameter.

An earlier draft drew the six-layer figure on four slides at four different sizes
— default × 648, then `height=520`, then `height=552`, then `height=552` with
`width=1200`. Each value was driven by what else that slide had to fit
underneath, which is defensible in isolation and wrong in sequence: the figure
jumped every time the speaker flipped, and "the same figure four times" was a
claim about structure rather than about pixels.

Two things changed. The figure now appears on **two** slides, not four — 41
introduces it, 43 recolours it — and `lichess_stack.draw()` takes tones and tags
but no width, height or origin. The constants in `decks/golem/lichess_stack.py`
are the geometry. Slides 44 and 45, which used to be the third and fourth
recolourings, carry the same argument in forms of their own. `fleet.py` holds its
own constants for the same reason: twelve wide frames over one fleet, and a box
that moved between them would read as a different fleet.

## Resolved: what language Emet resembles

**Emet is Elm-like.** Dr. Dub settled it, and it holds against `apps/emet/`:
`apps/emet/CLAUDE.md` describes the language as "modeled on Elm", the layout rule
is the offside rule, inference is Hindley-Milner with let-generalization, `case`
is checked for exhaustiveness and redundancy at compile time, records are
row-polymorphic with `{ r | f = v }` update, the operator fixity table is Elm's
exactly, the three constrained type variables are Elm's `number` / `comparable` /
`appendable` with no user typeclasses, and the module system and the stdlib
surface follow Elm down to the omissions — there is no `List.head`, and
`String.toInt : String -> Maybe Int`.

The differences worth saying out loud, if the comparison is pushed further than
the headline: there is no `|>` and no user-defined operators at all; there is no
`type alias`; the language is not total (ADR 0011 relaxed it, and the backstop is
a 20,000-frame depth counter); `Secretspec.get` is typed `String -> String` but
reads a real secret provider at compile time, so secret-ness is a dynamic taint
rather than a type; and there is no `Result`, `Dict`, `Set`, `Task`, `Cmd`, no
ports and no runtime, because the compiler evaluates the whole program and the
output is data.

The deck says it once, in slide 27's subtitle, and states only the hedged
headline — no specific similarity, because each of those is a claim needing its
own defence in a room that may contain an Elm user. It is deliberately not said
on 17 or 24, which name `.emet` files rather than describe the language; not on
23, where "a statically typed compiler" is the property being claimed against a
requirement; not on 33, 34, 36 or 37, where the lineage of the syntax is evidence
for nothing being graded; and not on 38, where question 4 asks the room to learn
Emet and the slide must carry nothing that needs explaining.

---

# The machine-lifecycle deck

## The argument

A lichess machine reaches service through five steps: order it from OVH, install
Debian, lay out the partition table, let Ansible install the basics, then
configure the services on it. **Four of those five are done by hand, and the one
tool covers the one in the middle.** The manual work is the majority of the
timeline rather than its edges, and drawing that shape honestly is most of what
the deck has to say.

The proposal is one tool per span: **Pulumi** takes 1 to 3, **Ansible keeps 4**,
**golem** takes 5. Ansible being kept is as much of the argument as the two
changes — this is not a rip-and-replace pitch, and golem is not trying to own
step 1.

**01 and 05 are one figure twice**, at identical geometry, and that is the deck's
spine. Five step boxes, each with its own mark and a number on a timeline axis
above it; three spans underneath saying who does each stretch. On 01 the spans
read *by hand · Ansible · by hand*, the by-hand ones dashed in the fleet frames'
notation. On 05 they read *Pulumi · Ansible · golem* at the same widths. The
grouping is the same in both, so the second slide is the first one answered.

**02 to 04 take the three spans of today one at a time.** 02 is the three panel
steps as icon cards, and states that none of them is in the Ansible repository —
step 4 is where the repository starts. 03 is the one step a tool owns, so it is
also where Ansible gets defined: a controller that runs an ordered list of steps
against a host over ssh, and a machine left with an Ansible border and no units
on it. 04 fills that machine with dashed cells and gives the inventory's own
counts.

**06 to 10 are the two tools that would take over.** 06 draws Pulumi as program,
engine, state and provider, with the arrow back from the provider drawn as well
as the arrow out. 07 names the resource and its fields. 08 is the slide that
keeps 07 honest. 09 draws golem only as far as this deck needs it — program,
manifest, one scroll per host, an agent on each host — because the other two
decks explain it at length. 10 closes on the artifact: the same three positions
drawn twice, with the file slot empty on the left.

## The ten slides

| # | Title | Module | Form | Mode |
|---|---|---|---|---|
| 01 | How a machine comes to exist today | `s01_today.py` | step band, spans | explanation |
| 02 | Steps 1 to 3: order, install, partition | `s02_order_install_partition.py` | icon cards | reference |
| 03 | Step 4: Ansible installs the basics | `s03_the_basics.py` | play, one host | explanation |
| 04 | Step 5: the services, by hand | `s04_the_services.py` | one machine, units | explanation |
| 05 | The proposal | `s05_the_proposal.py` | step band, spans | explanation |
| 06 | What Pulumi is | `s06_what_pulumi_is.py` | flow, state store | explanation |
| 07 | Steps 1 to 3 are one Pulumi resource | `s07_one_resource.py` | rows of field chips | reference |
| 08 | What the resource does not remove | `s08_where_a_person_stays.py` | three cards | reference |
| 09 | What golem is | `s09_what_golem_is.py` | pipeline, scrolls, hosts | explanation |
| 10 | What changes about steps 1 to 3 | `s10_what_changes.py` | before / after split | explanation |

## What the Pulumi OVHcloud provider actually covers

The weak claim was step 1, and it turned out to be the opposite of weak. Read
before drawing, from the provider's own reference:

- **Ordering is supported.** `ovh.Dedicated.Server` — Terraform's
  `ovh_dedicated_server`, which the Pulumi provider bridges — opens with "Use
  this resource to order and manage a dedicated server" and carries a section
  titled *Arguments used to order a dedicated server*: `ovhSubsidiary`, `plans`,
  `planOptions`, `range`. Supplying `serviceName` instead adopts a server that
  already exists rather than ordering one.
- **The OS install is on the same resource**, through `os` and `customizations`.
  `ovh.Dedicated.ServerReinstallTask` does it imperatively for a server already
  delivered; there is no `ServerInstallTask` any more.
- **Partitioning is on the same resource too**, under `storages` →
  `hardwareRaids` and `partitionings` → `layouts`, with `extras.lvs` and
  `extras.zps` for LVM and ZFS.

Three things bound the claim, and slide 08 says all three out loud:

- The `order_cart` family exists **only as data sources**. Plan codes are read,
  and a person picks which machine to buy. That is a decision the deck keeps
  deliberately, the same position it takes on placement.
- **The order is asynchronous.** The provider waits at most two hours for
  delivery; past that the apply ends in error while OVHcloud goes on delivering.
- **This is provider v2 and later.** v2.0.0 removed `ovh_me_installation_template`
  and its partition-scheme resources, and moved partitioning onto the server
  resource. Anyone who last looked before that release remembers a different API.

## The band's geometry is not a parameter

`decks/machine_lifecycle/lifecycle.py` holds the steps, their marks and every
coordinate. Slides 01 and 05 pass spans and nothing else. A step that moved
between the two frames, or a mark that changed, would make 05 read as a new
figure instead of as 01 answered — the same reason `lichess_stack.py` and
`fleet.py` keep their own constants.

Both slides pass the same `id_namespace`, so an unchanged element keeps its id
across the two documents. That is legal because they are separate documents and
the merge step renames collisions; it is not a mechanism anything relies on, for
the reasons under *Excalidraw+ transitions are unverified*.

## The Excalidraw wire format

Each of these cost time to discover once. `test_scenes.py` pins most of them.

**The document envelope.**

```json
{"type":"excalidraw","version":2,"source":"golem docs/presentation",
 "elements":[…],
 "appState":{"gridSize":null,"gridStep":5,"gridModeEnabled":false,
             "viewBackgroundColor":"#ffffff"},
 "files":{}}
```

**`files` is empty on every document but the three that carry the golem symbol** —
the title slide, the converged fleet frame, and the combined golem deck.
An embedded image is two halves: a `files` entry `{id, mimeType, dataURL,
created, lastRetrieved}` and an element with `type: "image"` plus `fileId`,
`status`, `scale` and `crop`. Excalidraw accepts an `image/svg+xml` data URL, and
an SVG data URL carries no intrinsic size, so the element's width comes from the
file's own `viewBox` ratio.

`created` and `lastRetrieved` are the generator's fixed `UPDATED`, never a clock —
Excalidraw itself writes `Date.now()` there, which would make every build differ
from the last. `test_scenes.py` asserts the constant, which is where that check
belongs: `restore()` hands `files` straight back, so asking the oracle whether the
map came back unchanged would only prove the object was passed through.

What the oracle can prove is reachability, and does: every image element's
`fileId` still resolves to an entry holding a data URL. An element whose file went
missing loads as a blank rectangle, and `fileId` round-trips either way.

**Every element carries the whole key set**: `id, type, x, y, width, height,
angle, strokeColor, backgroundColor, fillStyle, strokeWidth, strokeStyle,
roughness, opacity, groupIds, frameId, roundness, seed, version, versionNonce,
isDeleted, boundElements, updated, link, locked`. Text elements add `text,
fontSize, fontFamily, textAlign, verticalAlign, containerId, originalText,
lineHeight, autoResize`. Arrows and lines add `points, lastCommittedPoint,
startBinding, endBinding, startArrowhead, endArrowhead, elbowed`.

**`roundness`** is `{"type":3}` for rectangles, diamonds and ellipses,
`{"type":2}` for lines and arrows, and `null` for frames and sharp rectangles.

**`index` is omitted on purpose.** Excalidraw's `restore()` regenerates the
fractional index from array order. A hand-rolled one that is not strictly
increasing corrupts z-order rather than setting it. Array order *is* the z-order
it rebuilds from — append back-to-front.

**A label inside a shape is a bound text element**, not a shape with a `text`
property. The text element gets `containerId: <shape id>`; the shape gets
`boundElements: [{"type":"text","id":<text id>}]`. Both `text` and `originalText`
are set to the *already-wrapped* string, with hard newlines, so the layout
computed at build time is the layout on screen.

**An arrow's `points` are relative to its `x,y`**, so `points[0] == [0,0]`, and
`width`/`height` are the extents of the point bounding box. For an arrow
travelling up or left the visual box reaches back behind the anchor, so `x +
width` is not its right edge and `x, y` is not its top-left corner. Any
containment check has to derive a linear element's box from its points.

**A linear element's span must not be rounded.** Every other coordinate the
generator writes is rounded to two decimals; a linear element's `width` and
`height` cannot be, because Excalidraw recomputes them from the stored points and
keeps the full float. A curved arrow on the lifecycle slide spanned 57.72 down to
−14.04, was written as `71.76`, and came back from `restore()` as
`71.75999999999999` — an element rewritten on load, which the `restore()` oracle
catches and `test_scenes.py` could not, because it was measuring the output
against the same rounding that produced it. The span now goes in unrounded.

**Frames parent their children through `frameId`.** A frame is `type: "frame"`
with a `name`; every element inside it sets `frameId: <frame id>`; and the frame
is emitted before its children.

**`fontFamily` is 1 for the hand-drawn font and 3 for code.** Nothing else is
used.

## Stable ids across a sequence

Element ids come from `blake2s(id namespace + counter)`. The namespace defaults to
the scene key, so every slide gets its own — meaning the same machine box in two
frames would hold different ids.

The wide fleet frames share `fleet.ID_NAMESPACE` and `draw()` emits its base
elements in a fixed order, so an element that has not changed keeps one id right
through the sequence. **Nothing depends on this** — see below — and no further
machinery should be added for it.

The cost is one invariant elsewhere. Two elements sharing an id is legal only
while they are separate documents; the combined deck puts every frame on one
canvas, where a duplicate makes `restore()` reissue one at random and drop
whatever bindings pointed at it. `framed_deck` therefore claims ids as it merges:
the first frame in keeps its own, and a later collision is renamed along with
every `containerId` and `boundElements[].id` that referred to it.

## Excalidraw+ transitions are unverified

An earlier draft of this document claimed Excalidraw+'s Present mode interpolates
elements that persist between frames, and the shared id namespace was introduced
on that basis. **Treat the claim as unreliable.** Excalidraw's own presentations
documentation describes no transition behaviour; the interpolation claims are
third-party and none of them states what the matching key is. Element ids have to
be unique within a canvas, and `framed_deck` renames collisions as it merges the
deck, so ids cannot be shared across frames on the merged canvas at all — which is
a plausible reason nothing animates in practice.

**Every build-up is therefore carried by an additional static frame.** Where a
step would rely on the viewer noticing a change between two slides, the sequence
splits it so each frame shows one change and reads alone. Frames 05 and 07 exist
for exactly this reason, 11 to 14 page one Ansible play through four frames, and
17 to 20 split the golem pipeline four ways.

## The imported mark

`assets/robot-golem.svg` is golem's symbol: **by Lorc, from game-icons.net, under
CC BY 3.0**. Attribution is required, so it is credited in `assets/README.md`,
here, in `README.md`, and on every slide that draws it — the wording lives in
`decks/golem/golem_symbol.py` so there is one string to keep true.

The file is committed and read from disk. The build never fetches, and a mark that
arrived over the network at build time would make the output depend on a server.

It is used twice: on frame 01 at 280px and on frame 20 at 96px, each carrying the
credit line, because the licence requires attribution wherever the mark appears.
Beside the drawn marks a dense filled silhouette reads as a different medium,
which is fine for an identity mark and wrong for a vocabulary item. Everywhere a
mark has to be small or repeated — the per-machine agents on frames 18 to 21 —
the drawn vocabulary wins, because the imported one turns to a blob under about
40px and competes with the tones around it.

## Measuring text without a font

Nothing in the generator loads a font, so every width is an estimate — and the
estimate has to be told which font it is measuring.

`excalidraw/text.py` carries a per-character advance table calibrated for the
hand-drawn font. It charges 0.30–0.40em for the hairline and narrow characters,
which is right for prose and badly wrong for code: a monospace face gives about
0.62em to every character, and code literals are dense with exactly the
characters the table under-charges. Measuring mono text with hand metrics
under-measures it, Excalidraw re-wraps the literal on load, and the layout
computed at build time is not the layout on screen.

This was live, not hypothetical. A `golemctl plan --against-host` chip was
overflowing its container by 0.87px at the true monospace advance, and would have
wrapped the moment the file was opened. Measurement is now font-aware throughout
— `character_advance`, `line_advance`, `measured_width` and `wrapped` all take a
`font_family`.

`MONOSPACE_ADVANCE` is 0.65, deliberately above the true ~0.62. Erring high only
widens a chip; erring low wraps a code literal on load. Two tests pin mono labels
against the true 0.62 rather than against the generator's own constant, and they
check bound labels against Excalidraw's real bound-text padding of **5px** — not
`scene.CONTAINER_PADDING`, which is 12 and is slack for the width estimate, not a
match for the editor's number.

## The generated files are not in the repository

`dist/` is gitignored. The build is deterministic — no wall clock, no RNG, ids
and seeds from `blake2s(scene key + counter)` — so two builds of the same source
are byte-identical, and anyone can reproduce the exact bytes on demand. That is
the argument against carrying the generated JSON in the tree, not for it — a
fresh build is 366,596 lines across 87 files. `test_scenes.py` proves determinism by building twice into two temporary
directories and comparing, so nothing depends on a committed copy.
