# The talk

Thirteen slides answering three questions in order: what problem golem solves,
how it works, how to use it. Build and run instructions are in `README.md`; what
follows is the argument, what each slide has to say, and the format facts the
generator depends on.

## The argument

**One stack, and every hosting choice is a line drawn across it.** Slides 01 and
02 set that frame twice — first by what you *buy*, then by what you *configure* —
so the audience already has a shape in their head before any lichess detail
arrives. 02 ends by planting a flag: lichess is in the leftmost column,
configured rented bare metal, and everything after this is about that column.

**Then the stack gets specific.** 03 is the six-layer lichess figure. Layer 6 is
drawn as a column beside bands 2–5 rather than a band on top, because
orchestration acts *across* those layers. That drawing decision is itself an
argument, and 04 collects on it: layer 6 expanded is five separate jobs, none of
them optional, each answered by a platform, a script, or a human at a terminal.

**Three recolourings of one figure make the comparison honest.** 05 shows what
buying orchestration would cover — that is the yardstick. 06 shows where lichess
was with Ansible, and 07 shows December's push to containers. Because all three
are the *same* figure with different tones, the absence in 06 and 07 is visible
rather than asserted. 05 has to come first, or there is nothing for 06 and 07 to
be missing against.

**08 is the only detail slide before the pivot.** It shows how December actually
worked — vrack, dnsmasq, SRV records, `hosts.py`, quadlets — and names the three
things it could not do. It exists so that "where it broke" is about structure
rather than about a stack nobody has seen.

**09 must come before 10.** 09 is five problems, none of them a bug anyone could
have gone and fixed. 10 then pairs each requirement with the property that
answers it. Read after 09, every row on 10 is a debt being paid. Read before 09,
the same rows are a feature list, and the talk turns into a pitch. The order is
the argument.

**11, 12, 13 are the "how it works" third.** 11 is the pipeline end to end. 12 is
the authoring contract — a typed functional program and exactly four glyph kinds.
13 is the operator's view: two binaries, one wire, and plan-before-apply.

## The thirteen slides

Frame names in `dist/deck.excalidraw` are `NN · Title`; filenames are
`NN-slug.excalidraw`. Both derive from position in `SLIDE_MODULE_NAMES`.

### 01 · What you buy — `slides/s01_what_you_buy.py`

A service-model responsibility matrix, deliberately vendor-neutral — no AWS, GCP
or Azure. Rows top to bottom: Data, Application, Runtime & middleware, Operating
system, Virtualisation, Network & storage, Hardware, Facility & power. Columns:
Own hardware, Colocation, Rented bare metal, IaaS (cloud VMs), PaaS, SaaS.

Cells are `YOURS` or `THEIRS`, staircasing down the columns — own hardware is all
eight yours, colocation seven, rented bare metal the top five, IaaS the top four,
PaaS the top two, SaaS none. The exception is SaaS Data, which is `HOSTED`:
yours, but on their terms. Legend for the three tones, and a closing line — left
to right, the less you own the less you control; what you stop operating, you
start depending on.

### 02 · What you configure — `slides/s02_what_you_configure.py`

The same matrix shape, a different question. Columns: Bare metal + config mgmt,
Docker (one host), Swarm, Nomad, Kubernetes, Managed Kubernetes. Rows: App config
& secrets, Scaling policy, Service discovery & load balancing, Scheduling &
placement, Cluster membership, Container runtime, Host OS & kernel, Hardware.
Cells are `YOURS` (you configure it), `PLATFORM` (the platform answers it) or
`THEIRS` (not your problem).

Two annotations under the grid. A badge on the first column: **lichess is here**,
captioned "configured, rented bare metal". A bar spanning Docker through
Kubernetes: **Portainer — a configurator that sits on top of these, not another
layer of the stack**. Above the grid, the line the slide turns on: past the
middle you stop thinking in machines and start thinking in resources.

### 03 · What lichess runs — `slides/s03_lichess_stack.py`

The shared figure, drawn from `slides/lichess_stack.py`. Five bands drawn top to
bottom as 5, 4, 3, 2, 1, plus layer 6 as a tall column to the right spanning
bands 2–5. Layer 1 is the only band drawn full width — it runs under the column
too.

1. **Core OS, network, security** — Debian, kernel, users, sshd, nftables, the
   private network, TLS
2. **Application hosting** — container runtime (podman), systemd, storage and
   volumes, registry access
3. **Connective infrastructure** — DNS, SRV records, service discovery, reverse
   proxies, load balancers, the private network fabric
4. **Tools, dependencies, runtimes** — JVM, node, native libs, client libraries,
   base images
5. **The applications** — lila, lila-ws, lila-search, mongodb, redis, the rest
6. **Lifecycle / schedule / scaling** — the right-hand column, subdivided into
   the five parts of slide 04

`lichess_stack.draw()` takes per-layer and per-part `Tone` and tag mappings so
slides 05, 06 and 07 recolour this figure instead of redrawing it. That reuse is
the point of the whole generator: a band that moves moves on all four slides.

### 04 · What orchestration means — `slides/s04_orchestration.py`

Layer 6 expanded into five boxes. The substance, as specified:

- **Placement (the scheduler)** — the only part that answers *which node*
- **Lifecycle** — starting, stopping, restarting, draining, rolling updates,
  rollbacks
- **Health and reconciliation** — watching actual state, detecting drift or
  failure, rescheduling
- **Supporting plumbing** — networking, service discovery, load balancer
  registration, storage attachment, secrets and config distribution
- **Scaling** — adjusting replica counts in response to policy or load

The strings live once, in `ORCHESTRATION_PARTS` in `slides/lichess_stack.py`, and
are reused verbatim by slide 04 and by the column on 03, 05, 06 and 07. Only the
plumbing line is shortened for the canvas — "storage attachment, secrets and
config distribution" is drawn as "storage, secrets". Restore the full phrase if
the box ever gets taller.

Closing line: none of the five is optional. Every fleet answers all five — by a
platform, by a script, or by a human at a terminal.

### 05 · If we bought orchestration — `slides/s05_bought_orchestration.py`

The slide-03 figure recoloured. Nomad or Kubernetes covers layers 2, 3 and 6 and
every orchestration part inside 6; the OCI image carries 4 and 5; layer 1 stays
ours. A dashed callout hangs under layer 1: renting managed Kubernetes covers
layer 1 too — and costs more. Closing line: all five parts of layer 6 arrive
together — you configure the scheduler, the health loop and the plumbing, you do
not write them.

### 06 · Where we were: Ansible — `slides/s06_ansible.py`

The same figure, coloured by what a playbook could actually reach. Ansible
covered 1, 2 and 4. Layer 3 was mostly manual. Layers 5 and 6 were manual
outright, and every orchestration part with them — a human chose the host, a
human ran the change, and all five parts of orchestration lived in someone's
head.

### 07 · December: containers — `slides/s07_december_containers.py`

The figure a third time, this time with four owners rather than one:

- **quadlets (podman + systemd)** → layers 4 and 5
- **custom Python + Ansible** → layer 3, and the placement, plumbing and scaling
  parts of 6
- **systemd** → the lifecycle part of 6
- **Ansible** → layers 1 and 2
- health and reconciliation stayed manual, and layer 6 is tagged "no single
  owner"

Red `GAP` badges hang in the right-hand gutter off the parts that stayed
unsolved, under a "still unsolved" heading: **no move to another host** off
Placement, **no drain** and **no rollback** off Lifecycle. Closing line: quadlets
gave lifecycle on one host, placement and scaling were generated from a
hand-maintained table, so a human still made every decision.

### 08 · December: the plumbing — `slides/s08_december_plumbing.py`

Two box-and-arrow rows and a row of gaps.

Service discovery — a client resolves a service, not a machine: **OVH vrack** (a
private L2 between the rented machines) → **dnsmasq** (one resolver per host) →
**SRV records** (a name resolves to a host and a port) → **clients** resolve a
service, never a machine.

Placement and lifecycle: **`hosts.py`**, a hand-maintained placement table →
**Ansible inventory and quadlet variables** generated from it → **systemd
quadlets** written onto the host → **lifecycle**, whatever systemd gives you.

Then, in red, what this could not do: **drain**, **move a service to another
machine**, **roll back**. Closing line: placement lived in a Python file and in a
human's head, and nothing on the host knew what it was supposed to look like.

### 09 · Where it broke — `slides/s09_where_it_broke.py`

Five numbered problems. The substance:

1. **Ansible is imperative mutation** — idempotent by convention, not by
   construction
2. **No undo** — every rollback had to be written by hand, as another play
3. **No real static analysis** — `--check` collapses (a task that edits a file an
   earlier task created fails the whole dry run), so you end up putting the
   change onto a live host and finding the runtime errors there
4. **Hard to test a change against a known-good state** — no way to ask what a
   change would do to a host that was already good
5. **Dependent on the newest podman and Debian trixie** — the plumbing assumed
   the newest thing on every host

`--check` is drawn as a monospace chip on row 3 rather than inside the sentence.
Closing line, and the hinge into slide 10: none of this is Ansible's fault — it
is the cost of writing changes as steps instead of writing down the state you
want.

### 10 · What golem is, and is not — `slides/s10_what_golem_is.py`

Two panels at the top. **Not:** a replacement for bare-metal provisioning, OS
installation, or the basics of networking and security — layer 1 stays where it
is. **Is:** a replacement for the custom Python and the new Ansible being built
in December and January.

Below them, seven requirement → property rows, captioned "what you need" and "the
property that answers it":

| what you need | the property that answers it |
|---|---|
| describe the state you want | a declarative program, not a script |
| proper undo | every edit records its inverse; atomic rollback |
| drop it on any machine | a small statically linked binary (`golemd`) |
| assume nothing on the host | no interpreter, no runtime, no agent stack |
| catch mistakes before the host | a statically typed language and a compiler (`emetc`) |
| see the change before it happens | plan against the live host (`golemctl plan --against-host`) |
| move services safely | reversible revisions, so draining is a real operation |

Closing line: none of this is new orchestration — it is the same work, written
down as state instead of as steps, and reversible when it is wrong.

### 11 · The pipeline — `slides/s11_pipeline.py`

A row of five stages across the top: `fleet.emet` → `emetc build <source>` →
**manifest** (binary, content-addressed) → `golemctl apply` (`POST /manifest`) →
**golemd** on the host. An arrow drops from `golemd` into a panel, "Inside golemd
— one apply", holding two columns.

Plan: `AddressedScroll { content_id, scroll }` — golemd selects this host's
scroll by name; `reconcile::plan(prior, desired) -> Vec<GlyphOp>` keyed by
`Glyph::key()`, the diff being by content id so the same id means no work; and
`GlyphOp` = `Install | Remove | Replace | Noop`, four ops with no fifth.

Enact: `Reconciler::apply(&Glyph, ContentId) -> Outcome` with `Outcome { op, cid,
inverse, changed }` — apply captures the prior state as an `Inverse`, carried on
the `Outcome`; `Revision { id, created_at, kind, scroll_content_id, outcomes }`
with `kind: RevisionKind = Init | Reconcile`, the append-only journal of what
golem actually did; and `Reconciler::reverse(&Outcome)`, which replays that
`Outcome` to restore the prior state exactly.

A footer card states the manifest exactly: `Manifest { format_version,
emet_version, scrolls: Vec<AddressedScroll> }` with `FORMAT_VERSION = 5`, and
`ContentId` as a 32-byte BLAKE3 digest over postcard bytes — one per scroll, one
per glyph.

### 12 · Emet and the four glyphs — `slides/s12_emet_glyphs.py`

Left column: `main : List Scroll` — one Scroll per host, one program for the
fleet. A Scroll forks into a **branch** (named sub-scrolls) or a **leaf unit**
(glyphs, and an optional policy) — each level holds either glyphs or named
sub-scrolls, never both. A leaf unit is the failure-isolation boundary: one
unit's failure never rolls back a sibling.

Right column: the four glyph kinds, each with its Emet spelling, its Rust
constructor and its `Glyph::key()` prefix.

- `aptPackage { name }` → `Glyph::AptPackage { name }`, key `apt:<name>`
- `systemdService { unit }` → `Glyph::SystemdService { unit }`, key
  `systemd:<unit>`
- `file` / `directory` / `symlink` → one `Glyph::Filesystem { path, entry: Entry
  }`, key `file:<path>`, where `Entry = File { contents, perms } | Directory {
  perms } | Symlink { target }` and `Perms { mode: u16, owner: Option<String>,
  group: Option<String> }`. Three surface spellings of one glyph — the count
  stays four, and each arm carries only its own fields, so a symlink with a mode
  cannot be written.
- `lineInFile { path, line }` → `Glyph::LineInFile { path, line }`, key
  `fileline:<path>:<line>`

Two callouts carry the points that matter most. Richer shapes — workloads,
quadlets, ingress — are Emet library abstractions that compile down to these
four; golemd never grows a fifth kind. And: the four require systemd, but they do
not assume quadlets, so an older or different machine can be given another
approach. Closing bar: four glyph kinds is the whole contract between the
language and the daemon.

### 13 · Plan before apply — `slides/s13_golemctl_golemd.py`

Left panel, the author's machine: `golemctl apply <source> <addr>` (`--json`,
`--reattach`), `plan` (`--json`, `--detail`, `--against-host`), `state`,
`history`, `show`, and `golemctl fleet apply | plan | status` (`--inventory`,
`--hosts`) — exactly three fleet verbs, with no fleet `state`, `history` or
`show`.

Right panel, the host: golemd's eight routes, each glossed — `POST /manifest`,
`POST /plan?against_host=true`, `GET /reconciles/latest`,
`GET /reconciles/:id?after=<seq>`, `GET /state`, `GET /revisions`,
`GET /revisions/:id`, `GET /status`. Underneath, the apply handshake as three
chips: `POST /manifest` → `202 {"reconcile_id": <u64>}` → `GET /reconciles/:id`.

Across the middle, the claim from ADR 0058: **the plan reads the host — and only
a verdict crosses the port.** Below it the plan loop in four steps: `golemctl
plan --against-host` → `POST /plan?against_host=true` with `PlanScope =
JournalAndHost` (the host read is opt-in) → `observe(&[GlyphOp]) -> Observations`,
golemd probing dpkg, `/etc` and systemd → `Observation` = `Realized | Divergent |
Absent | Unknown(Unknowable)` with `Unknowable = Sealed | Unreadable |
NotModelled` — a verdict, never contents, mode, owner or dpkg status.

A footnote carries the conflict codes: `409 HostBusy` for an `--against-host`
plan racing a live apply, `409 ReconcileInProgress` for an apply racing an apply,
and a plain `golemctl plan` still working during an apply.

The routes, verbs and flags on this slide are quoted from shipped code and go
stale when it moves. Check them against
`sites/website/src/content/docs/reference/cli.mdx` and
`docs/adr/0058-the-plan-reads-the-host-and-only-a-verdict-crosses-the-port.md`.

## Two corrections against the code

The earlier drafts of slide 11 got these wrong. Both are drawn correctly now;
they are recorded so nobody reintroduces the error.

**`Init` and `Reconcile` are variants of `RevisionKind`, not of `Revision`.**
`Revision` is a struct — `{ id, created_at, kind, scroll_content_id, outcomes }`
— and `kind: RevisionKind` is where the two variants live
(`apps/golemd/src/journal.rs`).

**`Inverse` is a field of `Outcome`, not an argument.**
`Reconciler::apply(&Glyph, ContentId) -> Outcome` returns `Outcome { op, cid,
inverse, changed }`, and `Reconciler::reverse(&Outcome)` takes the whole
`Outcome` and reads `outcome.inverse` from it (`apps/golemd/src/reconcilers.rs`).

## Open questions

**What language family Emet actually resembles.** The original spec described
Emet as "inspired by (nearly identical to) emet", which defines the language in
terms of itself and cannot be what was meant. A later draft rendered it as
"ML-family", which is plausible but unverified. The shipped slide 12 makes no
claim at all — its subtitle is only "A typed, functional program evaluates to one
Scroll per host" — so nothing false is drawn. The question is still open, and
needs an answer before the talk is given if the comparison is going to be made
out loud.

**Whether the shared figure should be pixel-identical across its four slides.**
It is not, today. Slide 03 draws it at the default `CONTENT_WIDTH` × 648. Slide
05 passes `height=520`, slide 06 `height=552`, and slide 07 passes both
`height=552` and `width=1200` — the narrower width opening a right-hand gutter
for the red `GAP` badges that hang off the orchestration parts. Each value is
driven by what else that slide has to fit under the figure, which is defensible,
but it does mean the "same figure four times" claim is about structure and
colour rather than geometry. Whether the four should be forced to match exactly
is a judgement call, not a bug.

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

**Frames parent their children through `frameId`.** A frame is `type: "frame"`
with a `name`; every element inside it sets `frameId: <frame id>`; and the frame
is emitted before its children.

**`fontFamily` is 1 for the hand-drawn font and 3 for code.** Nothing else is
used.

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

This was live, not hypothetical. Slide 10's `golemctl plan --against-host` chip
was overflowing its container by 0.87px at the true monospace advance, and would
have wrapped the moment the file was opened. Measurement is now font-aware
throughout — `character_advance`, `line_advance`, `measured_width` and `wrapped`
all take a `font_family`.

`MONOSPACE_ADVANCE` is 0.65, deliberately above the true ~0.62. Erring high only
widens a chip; erring low wraps a code literal on load. Two tests pin mono labels
against the true 0.62 rather than against the generator's own constant, and they
check bound labels against Excalidraw's real bound-text padding of **5px** — not
`scene.CONTAINER_PADDING`, which is 12 and is slack for the width estimate, not a
match for the editor's number.
