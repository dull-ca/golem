# The talks

Two decks. **golem** answers three questions in order — what problem golem
solves, how it works, how to use it. **orchestration** is a standalone primer on
what a cluster actually does, built from icons, landing on the one job golem
deliberately does not do.

Build and run instructions are in `README.md`; what follows is the argument, what
each slide has to say, and the format facts the generator depends on.

---

# The golem deck

## The argument

**One stack, and every hosting choice is a line drawn across it.** 01 and 02 set
that frame twice — first by what you *buy*, then by what you *configure* — so the
audience already has a shape in their head before any lichess detail arrives. 03
plants the flag: on a ladder from machines to resources, lichess is on the
leftmost rung, and everything after this is about that rung. 03 also parks
Portainer where it belongs — a configurator sitting on top of the middle four,
not another layer of the stack.

**Then the stack gets specific.** 04 is the six-layer lichess figure. Layer 6 is
drawn as a column beside bands 2–5 rather than a band on top, because
orchestration acts *across* those layers. That drawing decision is itself an
argument, and 05 collects on it: layer 6 expanded is five separate jobs, none of
them optional, each answered by a platform, a script, or a human at a terminal.

**The figure is shown twice, not four times.** 06 recolours it to show what
buying orchestration would cover — the yardstick. That is the only recolour, and
it is drawn at geometry identical to 04, so flipping between the two changes
colour and nothing else. Where we actually were is then shown by *different*
forms, because four recolourings of one figure read as one slide shown four
times: 07 measures Ansible's reach as coverage bars, layer by layer, and 08 names
the four tools that shared December's layers and the one job nobody took.

**09, 10 and 11 are December in detail.** They exist so that "where it broke" is
about structure rather than about a stack nobody has seen. 09 is service
discovery, 10 is placement and lifecycle, 11 is the three operations that were
simply missing.

**12 must come before 13.** 12 is five problems, none of them a bug anyone could
have gone and fixed. 13 and 14 then pair each requirement with the property that
answers it. Read after 12, every row on 14 is a debt being paid. Read before 12,
the same rows are a feature list, and the talk turns into a pitch. The order is
the argument.

**15 to 22 are the "how it works" third.** 15 is the pipeline end to end, 16 the
diff, 17 apply and undo. 18 and 19 are the authoring contract — a typed
functional program, and exactly four glyph kinds. 20, 21 and 22 are the
operator's view: two binaries, one wire, and plan-before-apply.

## The twenty-two slides

Frame names in `dist/golem/golem-deck.excalidraw` are `NN · Title`; filenames are
`NN-slug.excalidraw`. Both derive from position in `SLIDE_MODULE_NAMES` in
`decks/golem/__init__.py`.

### 01 · What you buy — `s01_what_you_buy.py` — *matrix*

A service-model responsibility matrix, deliberately vendor-neutral — no AWS, GCP
or Azure. Rows top to bottom: Data, Application, Runtime & middleware, Operating
system, Virtualisation, Network & storage, Hardware, Facility & power. Columns:
Own hardware, Colocation, Rented bare metal, IaaS (cloud VMs), PaaS, SaaS.

Cells staircase down the columns — own hardware is all eight yours, colocation
seven, rented bare metal the top five, IaaS the top four, PaaS the top two, SaaS
none. The exception is SaaS Data, which is *hosted*: yours, but on their terms.

Cells carry colour and no text. An earlier draft wrote `YOURS` / `THEIRS` into
all forty-eight of them; at the type floor that is forty-eight words of visual
noise saying what three legend swatches say once. Closing line: what you stop
operating, you start depending on.

### 02 · What you configure — `s02_what_you_configure.py` — *matrix*

The same matrix shape, a different question, and the second and last matrix in
this deck. Columns: Bare metal + config mgmt, Docker (one host), Swarm, Nomad,
Kubernetes, Managed Kubernetes. Rows: App config & secrets, Scaling policy,
Service discovery & load balancing, Scheduling & placement, Cluster membership,
Container runtime, Host OS & kernel, Hardware. Three tones: yours, the platform
answers it, not your problem.

### 03 · Where lichess sits — `s03_where_lichess_sits.py` — *timeline*

The six columns of 02 as rungs on an axis. A badge over the first rung: **lichess
is here**. A bar spanning Docker through Kubernetes: **Portainer — a configurator
on top of these, not another layer**. Below the axis, a before/after split on the
line the slide turns on: left of the middle you name the machine; right of it you
name the shape, and the platform picks the machine.

### 04 · What lichess runs — `s04_lichess_stack.py` — *layered stack*

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
   the five parts of slide 05

Those band details are trimmed to one line at 24pt. The fuller enumerations they
came from are: layer 1 also users and the private network; layer 2 also volumes;
layer 3 also service discovery, reverse proxies and the private network fabric;
layer 4 also client libraries; layer 5 also "the rest". They are the speaker's
sentences now, not the slide's.

`lichess_stack.draw()` takes per-layer and per-part `Tone`s and tags but **no
geometry**. Slide 06 recolours this figure rather than redrawing it.

### 05 · What orchestration means — `s05_orchestration.py` — *radial hub*

Layer 6 as a hub with five satellites, the five parts named once in
`decks/vocabulary.py` and reused verbatim here, in the column on 04 and 06, and
throughout the orchestration deck:

- **Placement** — the scheduler, the only part that answers *which node*
- **Lifecycle** — start, stop, restart, drain, rolling update, rollback
- **Health and reconciliation** — watch actual state, detect drift or failure,
  reschedule
- **Supporting plumbing** — networking, service discovery, load balancers,
  storage, secrets
- **Scaling** — replica counts moved by policy or load

Closing line: none is optional — a platform, a script, or a human answers each.

### 06 · If we bought orchestration — `s06_bought_orchestration.py` — *layered stack*

The slide-04 figure recoloured, at identical geometry. Nomad or Kubernetes covers
layers 2, 3 and 6 and every orchestration part inside 6; the OCI image carries 4
and 5; layer 1 stays ours. A dashed callout: renting managed Kubernetes covers
layer 1 too — and costs more. The subtitle carries the point that used to be a
closing line: all five parts of layer 6 arrive together.

### 07 · Where we were: Ansible — `s07_ansible.py` — *coverage bars*

Six tracks, one per layer, each showing how far a playbook actually reached.
Ansible covered 1, 2 and 4 outright; layer 3 was mostly manual; layers 5 and 6
were manual, and layer 6's track is tagged *manual — all five parts*. Closing
line: all five parts of orchestration lived in someone's head.

Drawn as bars rather than as a third recolouring of the stack. The absence is the
whole point of the slide, and an empty track shows it more directly than a
different fill on the same rectangle — and it stops slides 04 through 08 reading
as one figure four times.

### 08 · December: who owned what — `s08_december_owners.py` — *card rhythm*

Five cards in a 3 / 2 / 1 rhythm: Ansible → layers 1 and 2; custom Python → layer
3; quadlets → layers 4 and 5; custom Python + Ansible → placement, plumbing and
scaling; systemd → lifecycle. Then, full width and red: **Nobody** → health and
reconciliation stayed manual. Closing line: layer 6 had no single owner.

### 09 · December: service discovery — `s09_december_discovery.py` — *icon cards*

Four icon-led cards with flow arrows: **OVH vrack** (network link) → **dnsmasq**
(DNS lookup) → **SRV records** (service) → **Clients** (container). Closing bar:
a client resolves a service, never a machine.

### 10 · December: placement and lifecycle — `s10_december_placement.py` — *icon cards*

Four more: **`hosts.py`** (binding — a human doing the binding by hand) →
**generated config** (registry) → **systemd quadlets** (container) →
**lifecycle** (host). Closing bar: a human chose the host, every time.

### 11 · December: what it could not do — `s11_december_gaps.py` — *icon cards*

Three, large and red, each carrying the mark for the operation that was missing:
**Drain** (drain), **Move a service** (binding), **Roll back** (rollback).
Closing bar: placement lived in a Python file and in a human's head.

### 12 · Where it broke — `s12_where_it_broke.py`

Five numbered problems, each a heading and one line:

1. **Ansible is imperative mutation** — idempotent by convention, not by
   construction
2. **No undo** — every rollback written by hand, as another play
3. **No real static analysis** — the dry run collapses, so runtime errors surface
   on a live host. `--check` is a monospace chip on the row rather than a word in
   the sentence
4. **No way to test against a known-good host** — nothing could answer what this
   change would do
5. **Tied to the newest podman and Debian trixie** — the plumbing assumed the
   newest thing everywhere

Closing line, and the hinge into 13: the cost of writing changes as steps.

### 13 · What golem is, and is not — `s13_what_golem_is.py` — *before / after split*

**Not:** a replacement for bare-metal provisioning, OS installation, or the
basics of networking and security. **Is:** a replacement for the custom Python
and the new Ansible being built in December and January. A bar between them:
layer 1 stays exactly where it is.

### 14 · What you need, and what answers it — `s14_requirement_and_property.py`

Seven requirement → property rows, captioned "what you need" and "the property
that answers it":

| what you need | the property that answers it |
|---|---|
| describe the state you want | a declarative program, not a script |
| proper undo | every edit records its inverse |
| drop it on any machine | a small statically linked binary (`golemd`) |
| assume nothing on the host | no interpreter, no runtime, no agent |
| catch mistakes before the host | a statically typed compiler (`emetc`) |
| see the change before it happens | plan against the live host (`golemctl plan --against-host`) |
| move services safely | reversible revisions, so drain is real |

### 15 · The pipeline — `s15_the_pipeline.py` — *swimlane pipeline*

Five stages: `fleet.emet` → `emetc build` → **manifest** → `golemctl apply` →
**golemd** on the host. Below them the manifest stated exactly: `Manifest {
format_version, emet_version, scrolls: Vec<AddressedScroll> }` with
`FORMAT_VERSION = 5`, `AddressedScroll { content_id, scroll }`, and `ContentId` as
a 32-byte BLAKE3 digest over postcard bytes, one per scroll and one per glyph.
Closing bar: same content id, no work.

### 16 · Inside golemd: the diff — `s16_the_diff.py` — *before / after split*

Two panels, **prior** and **desired**, each holding `AddressedScroll {
content_id, scroll }` — golemd selects this host's scroll by name. An arrow drops
into `reconcile::plan(prior, desired) -> Vec<GlyphOp>`, keyed by `Glyph::key()`,
and that into four op chips: `Install`, `Remove`, `Replace`, `Noop`. Closing bar:
four operations, there is no fifth. Footnote: the diff is by content id, so the
same id means no work.

### 17 · Inside golemd: apply and undo — `s17_apply_and_undo.py` — *loop*

Three cards and a return arrow that closes the loop.
`Reconciler::apply(&Glyph, ContentId) -> Outcome` with `Outcome { op, cid,
inverse, changed }` — apply captures the prior state as an `Inverse`, carried on
the `Outcome`. Then `Revision { id, created_at, kind, scroll_content_id, outcomes
}` with `kind: RevisionKind = Init | Reconcile`, the append-only journal of what
golem actually did. Then `Reconciler::reverse(&Outcome)`, which replays that
`Outcome` to restore the prior state exactly. Closing bar: golem only ever
reverses edits it recorded.

### 18 · One program, one scroll per host — `s18_the_scroll_tree.py` — *tree*

`main : List Scroll`, then a Scroll forking into a **branch** (named sub-scrolls)
or a **leaf unit** (glyphs, and an optional policy). Line under it: either glyphs
or named sub-scrolls at each level — never both. Two callouts: a leaf unit is the
failure-isolation boundary, one unit's failure never rolls back a sibling; and
workloads, quadlets and ingress are Emet libraries that compile down to the four
glyphs.

### 19 · The four glyphs — `s19_the_four_glyphs.py`

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

The subtitle carries the point: each arm carries only its own fields, so illegal
states cannot be written.

### 20 · golemctl — the verbs — `s20_golemctl_verbs.py`

Five host verbs — `apply` (`--json`, `--reattach`), `plan` (`--json`,
`--detail`, `--against-host`), `state`, `history`, `show` — then `golemctl fleet
apply | plan | status` (`--inventory`, `--hosts`), exactly three fleet verbs with
no fleet `state`, `history` or `show`. Underneath, the apply handshake as three
chips: `POST /manifest` → `202 {"reconcile_id": <u64>}` → `GET /reconciles/:id`.

### 21 · golemd — the routes — `s21_golemd_routes.py`

The eight routes, each glossed: `POST /manifest`, `POST /plan?against_host=true`,
`GET /reconciles/latest`, `GET /reconciles/:id?after=<seq>`, `GET /state`,
`GET /revisions`, `GET /revisions/:id`, `GET /status`. Then the conflict codes as
three badges: `409 HostBusy` (a host-reading plan hit a live apply),
`409 ReconcileInProgress` (an apply hit an apply), and a plain plan that never
blocks.

### 22 · Plan before apply — `s22_plan_against_host.py`

The claim from ADR 0058 across the top: **the plan reads the host — and only a
verdict crosses the port.** Below it the plan loop in four steps: `golemctl plan
--against-host` → `POST /plan?against_host=true` with `PlanScope =
JournalAndHost` (the host read is opt-in) → `observe(&[GlyphOp]) -> Observations`,
golemd probing dpkg, `/etc` and systemd → `Observation = Realized | Divergent |
Absent | Unknown(Unknowable)` with `Unknowable = Sealed | Unreadable |
NotModelled` — a verdict, never contents, mode, owner or dpkg status.

The routes, verbs and flags on 20, 21 and 22 are quoted from shipped code and go
stale when it moves. Check them against
`sites/website/src/content/docs/reference/cli.mdx` and
`docs/adr/0058-the-plan-reads-the-host-and-only-a-verdict-crosses-the-port.md`.

---

# The orchestration deck

## The argument

A cluster is not a new kind of thing; it is a pile of ordinary things with one
decision added. The deck builds that pile from the bottom, one idea per slide,
each carrying its own mark so the vocabulary is learned by seeing it rather than
by being told.

**01 to 05 are one machine.** A process on a host shares one filesystem, one
network and one set of library versions with everything else on the box; a
container is still a process on that host, with an image, namespaces and cgroups
added; the image is layered, content-addressed and immutable; the registry is the
only thing a host has to reach; and the container runtime answers everything
about *this* machine and nothing about which machine.

**06 is the hinge.** Many hosts means a control plane, workers and a
desired-state store — and from there on you stop naming machines.

**07 names the five jobs**, using the same five names as slide 05 of the golem
deck, because they are the same five jobs. 08 through 15 then take four of them
one at a time: placement (08, 09), lifecycle (10), health and reconciliation
(11), and supporting plumbing spread across connectivity (12, 13), scaling (14)
and storage and secrets (15).

**08 is the slide the deck exists for.** Placement is the only part that answers
*which node*, and it is drawn as an act: an unplaced workload, the candidate
nodes, and the binding that settled it.

**16 and 17 land it.** 16 is one matrix across Docker, Swarm, Nomad and
Kubernetes — who provides which piece. 17 says where golem sits: declarative
desired state plus reversible enactment, and deliberately **not** a scheduler.
That boundary is the point, not an omission.

## The seventeen slides

| # | Title | Module | Form |
|---|---|---|---|
| 01 | A process on a host | `s01_a_process_on_a_host.py` | split / cards |
| 02 | What a container adds | `s02_what_a_container_adds.py` | icon cards |
| 03 | The image | `s03_the_image.py` | hub / stack |
| 04 | Registry, pull, run | `s04_registry_pull_run.py` | icon cards, flow |
| 05 | One host, many containers | `s05_one_host_many_containers.py` | host with workloads |
| 06 | Many hosts: the cluster | `s06_many_hosts_the_cluster.py` | cluster map |
| 07 | The five jobs | `s07_the_five_jobs.py` | numbered stack |
| 08 | Placement: the binding | `s08_placement_the_binding.py` | binding mark |
| 09 | What the scheduler weighs | `s09_placement_what_it_weighs.py` | hub / cards |
| 10 | Lifecycle | `s10_lifecycle.py` | state machine |
| 11 | Health and reconciliation | `s11_health_and_reconciliation.py` | loop |
| 12 | Connectivity: addressing | `s12_connectivity_addressing.py` | before / after split |
| 13 | Connectivity: the service | `s13_connectivity_the_service.py` | icon cards, flow |
| 14 | Scaling | `s14_scaling.py` | replica set |
| 15 | Storage and secrets | `s15_storage_and_secrets.py` | split |
| 16 | Who provides which piece | `s16_who_provides_which_piece.py` | matrix |
| 17 | Where golem sits | `s17_where_golem_sits.py` | before / after split |

Slide 16's shape, and why it is worth drawing: Docker on one host leaves almost
everything to you; Swarm and Kubernetes answer almost all of it; Nomad answers
most of it but leaves supporting plumbing and secrets to Consul and Vault. That
asymmetry is the information.

Slide 17 makes exactly two claims, and no more. golem is declarative desired
state and reversible enactment — every edit records its inverse, so a change can
be taken back exactly. golem is not a scheduler: nothing in it answers which
node, and the program names the host the way a person would.

---

## Two corrections against the code

The earlier drafts got these wrong. Both are drawn correctly now; they are
recorded so nobody reintroduces the error.

**`Init` and `Reconcile` are variants of `RevisionKind`, not of `Revision`.**
`Revision` is a struct — `{ id, created_at, kind, scroll_content_id, outcomes }`
— and `kind: RevisionKind` is where the two variants live
(`apps/golemd/src/journal.rs`).

**`Inverse` is a field of `Outcome`, not an argument.**
`Reconciler::apply(&Glyph, ContentId) -> Outcome` returns `Outcome { op, cid,
inverse, changed }`, and `Reconciler::reverse(&Outcome)` takes the whole
`Outcome` and reads `outcome.inverse` from it (`apps/golemd/src/reconcilers.rs`).

## Resolved: the shared figure's geometry

It is now one geometry, and it is not a parameter.

An earlier draft drew the six-layer figure on four slides at four different sizes
— default × 648, then `height=520`, then `height=552`, then `height=552` with
`width=1200`. Each value was driven by what else that slide had to fit
underneath, which is defensible in isolation and wrong in sequence: the figure
jumped every time the speaker flipped, and "the same figure four times" was a
claim about structure rather than about pixels.

Two things changed. The figure now appears on **two** slides, not four — 04
introduces it, 06 recolours it — and `lichess_stack.draw()` takes tones and tags
but no width, height or origin. The constants in `decks/golem/lichess_stack.py`
are the geometry. Slides 07 and 08, which used to be the third and fourth
recolourings, carry the same argument in forms of their own.

## Open question

**What language family Emet actually resembles.** The original spec described
Emet as "inspired by (nearly identical to) emet", which defines the language in
terms of itself and cannot be what was meant. A later draft rendered it as
"ML-family", which is plausible but unverified. Slide 18 makes no claim at all —
its subtitle is only "A typed, functional program evaluates to a list of scrolls"
— so nothing false is drawn. The question is still open, and needs an answer
before the talk is given if the comparison is going to be made out loud.

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
the argument against carrying 47,000 lines of generated JSON in the tree, not for
it. `test_scenes.py` proves determinism by building twice into two temporary
directories and comparing, so nothing depends on a committed copy.
