# The talks

Two decks. **golem** answers three questions in order — what problem golem
solves, how it works, how to use it. **orchestration** is a standalone primer on
what a cluster does, built from icons, landing on the one job golem deliberately
does not do.

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
the canvas. The hub of slide 05 once read `Layer 6 / one word` — a fragment
snapped off that slide's own subtitle — where it had to read **Orchestration**.

The first time a term appears it gets one flat sentence saying what it is: an
image is read-only filesystem layers plus a config, named by the digest of its
contents; reconciliation compares the state you asked for against the state on the
host and acts on the difference. Not an evocation of it.

---

# The golem deck

## The argument

**One stack, and every hosting choice is a line drawn across it.** 01 and 02 set
that frame twice — first by what you *buy*, then by what you *configure* — so the
audience already has a shape in their head before any lichess detail arrives. 03
plants the flag: on a ladder from machines to resources, lichess is on the
leftmost rung, and everything after this is about that rung. 03 also parks
Portainer where it belongs — a web UI over the middle four, sitting on top of the
ladder rather than forming a rung of it.

**Then the stack gets specific.** 04 is the six-layer lichess figure. Layer 6 is
drawn as a column beside bands 2–5 rather than a band on top, because
orchestration acts *across* those layers. That drawing decision is itself an
argument, and 05 collects on it: layer 6 expanded is five separate jobs, each one
done by a platform, by a script, or by a person.

**The figure is shown twice, not four times.** 06 recolours it to show what
buying orchestration would cover — the yardstick. That is the only recolour, and
it is drawn at geometry identical to 04, so flipping between the two changes
colour and nothing else. Where we were is then shown by *different* forms,
because four recolourings of one figure read as one slide shown four times: 07
measures Ansible's coverage as bars, layer by layer, and 08 names the four tools
that shared December's layers and the one job nobody took.

**09, 10 and 11 are December in detail.** They exist so that "where it broke" is
about structure rather than about a stack nobody has seen. 09 is service
discovery, 10 is placement and lifecycle, 11 is the move that took four
hand-ordered steps.

**A person choosing the host was never the defect.** golem keeps that choice — 13
and 14 here, and 17 of the orchestration deck, all say so — so no slide may frame
manual placement as a deficiency golem outgrew. What December lacked was
mechanism: no drain, no rollback, no preview, and no way to say *this service now
runs on host B* as one change. 11 names that last one and only that one.

**12 must come before 14.** 12 is five problems in how changes were written and
applied; 14 then pairs each requirement with the property that meets it. Read
after 12, every row on 14 is one of those problems being answered. Read before 12,
the same rows are a feature list, and the talk turns into a pitch. The order is
the argument, and it is carried by the sequence rather than by a strapline saying
so — 14's subtitle used to announce it, which is the speaker's job.

**15 to 22 are the "how it works" third.** 15 is the pipeline end to end, 16 the
diff, 17 apply and undo. 18 and 19 are the authoring contract — a typed
functional program, and exactly four glyph kinds. 20, 21 and 22 are the
operator's view: two binaries, one wire, and plan-before-apply.

## The twenty-two slides

Frame names in `dist/golem/golem-deck.excalidraw` are `NN · Title`; filenames are
`NN-slug.excalidraw`. Both derive from position in `SLIDE_MODULE_NAMES` in
`decks/golem/__init__.py`.

### 01 · What you buy — `s01_what_you_buy.py` — *matrix* — **reference**

A service-model responsibility matrix, deliberately vendor-neutral — no AWS, GCP
or Azure. Rows top to bottom: Data, Application, Runtime & middleware, Operating
system, Virtualisation, Network & storage, Hardware, Facility & power. Columns:
Own hardware, Colocation, Rented bare metal, IaaS (cloud VMs), PaaS, SaaS.

Cells staircase down the columns — own hardware is all eight yours, colocation
seven, rented bare metal the top five, IaaS the top four, PaaS the top two, SaaS
none. The exception is SaaS Data, which is hosted.

Cells carry colour and no text. An earlier draft wrote `YOURS` / `THEIRS` into
all forty-eight of them; at the type floor that is forty-eight words of visual
noise saying what three legend swatches say once. The legend is the whole of the
prose: *you operate it* / *the provider operates it* / *yours, stored by the
provider*. No subtitle, no closing line — the column headers name the service
models and the rows name the stack, so a strapline would only restate them.

### 02 · What you configure — `s02_what_you_configure.py` — *matrix* — **reference**

The same matrix shape, a different question, and the second and last matrix in
this deck. Columns: Bare metal + config mgmt, Docker (one host), Swarm, Nomad,
Kubernetes, Managed Kubernetes. Rows: App config & secrets, Scaling policy,
Service discovery & load balancing, Scheduling & placement, Cluster membership,
Container runtime, Host OS & kernel, Hardware. Three tones, and again the only
prose on the slide: *you configure it* / *the platform provides it* / *not yours
to configure*.

### 03 · Where lichess sits — `s03_where_lichess_sits.py` — *timeline* — **explanation**

The six columns of 02 as rungs on an axis. A badge over the first rung: **lichess
is here**. A bar spanning Docker through Kubernetes: **Portainer — a web UI that
manages these platforms**.

Below the axis, a split whose two headings are the claim and whose bodies are the
columns each covers: **You name the machine** — bare metal with configuration
management, and Docker on one host; **The platform picks the machine** — Swarm,
Nomad, Kubernetes, managed Kubernetes. The headings used to read "Left of the
middle" and "Right of the middle", which named a position on the canvas rather
than a thing, and left the two bodies to carry the actual point.

### 04 · What lichess runs — `s04_lichess_stack.py` — *layered stack* — **reference**

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

The only prose is one line under the figure, explaining the shape a reader is
looking at: layer 6 runs across layers 2 to 5, so it is drawn beside them.

### 05 · What orchestration means — `s05_orchestration.py` — *radial hub* — **reference**

A hub labelled **Orchestration** — *layer 6 of the stack* — with five satellites,
the five parts named once in `decks/vocabulary.py` and reused verbatim here, in
the column on 04 and 06, and throughout the orchestration deck:

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
the gloss, and the only-part claim is a closing line on slide 08 of the
orchestration deck, where it belongs.

Closing line: each of these is done by a platform, a script, or a person. A person
doing one is an answer, not a failure.

### 06 · If we bought orchestration — `s06_bought_orchestration.py` — *layered stack* — **explanation**

The slide-04 figure recoloured, at identical geometry. Nomad or Kubernetes covers
layers 2, 3 and 6 and every orchestration part inside 6; the OCI image supplies 4
and 5; layer 1 is ours. A dashed callout: renting managed Kubernetes covers layer
1 too — and costs more. Subtitle: a platform provides all five parts of layer 6
together. The legend reads *provided by the platform* / *provided by the image* /
*ours to operate* — an image does not "carry" anything.

### 07 · What Ansible managed — `s07_ansible.py` — *coverage bars* — **reference**

Six tracks, one per layer. Ansible managed 1, 2 and 4 outright; layer 3 was mostly
by hand; layers 5 and 6 were by hand, and layer 6's track is tagged *by hand — all
five parts*. Legend: *managed by Ansible* / *done by hand*.

The tags used to read "Ansible reached it", "still ours" and "a human decided and
did it" — an invented idiom that made a playbook an agent with reach, and made a
person deciding sound like a fault. The bars and the legend now carry the whole
slide; there is no closing line.

Drawn as bars rather than as a third recolouring of the stack. The absence is the
point of the slide, and an empty track shows it more directly than a different
fill on the same rectangle — and it stops slides 04 through 08 reading as one
figure four times.

### 08 · December: who owned what — `s08_december_owners.py` — *card rhythm* — **reference**

Five cards in a 3 / 2 / 1 rhythm: Ansible → layers 1 and 2; custom Python → layer
3; quadlets → layers 4 and 5; custom Python + Ansible → placement, plumbing and
scaling; systemd → lifecycle. Then, full width and red: **Nobody** → nothing
watched for drift or failure. Closing line: layer 6 had no single owner.

### 09 · December: how a client found a service — `s09_december_discovery.py` — *icon cards* — **explanation**

Four icon-led cards with flow arrows: **OVH vrack** (network link) → **dnsmasq**
(DNS lookup) → **SRV records** (service) → **Clients** (container). The cards make
the point end to end, so the closing bar is gone; the same claim, phrased flatly,
is the closing line of slide 13 of the orchestration deck.

### 10 · December: how a service reached a host — `s10_december_placement.py` — *icon cards* — **explanation**

Four more: **`hosts.py`** (binding — which service runs on which host, written
down) → **generated config** (registry) → **systemd quadlets** (container) →
**lifecycle** (host). Closing bar: a person chose which host ran each service.

That line used to read "A human chose the host. Every time." — the same fact
delivered as an indictment, and it contradicted slide 17 of the orchestration
deck. The fact stays; the verdict goes. Slide 11 names the defect that was real.

### 11 · December: moving a service — `s11_december_moving_a_service.py` — *numbered steps* — **explanation**

The four hand-ordered steps a move took: **1** edit the definition, marking the
service disabled → **2** apply to host A, where it stops and uninstalls → **3**
edit again, removing it from A and adding it to B → **4** apply, and it installs
on B. Note: out of order, it runs on both hosts or on neither.

Then what golem changes, and what it does not: **in golem this is one edit and one
apply** — change which host the service belongs to, and B installs it while A
removes it, both falling out of the same manifest diffed per host. Below that, in
the gap tone: **nothing orders the two, so both or neither may be running
briefly.**

That second line is load-bearing and must not be dropped. golem ships no
cross-host ordering: `golemctl fleet` spawns one task per target with no barrier
between them (`apps/golemctl/src/fleet.rs`), and no ADR or TODO proposes
otherwise. The improvement this slide draws is expressibility — three
hand-sequenced edits collapsing to one — and never an orchestrated cutover.

This slide replaced *December: what it could not do*, which listed Drain, Move a
service and Roll back as three missing operations. Two of those survive elsewhere
— "No undo" is problem 2 on slide 12 — and the third was the interesting one, but
its old gloss ("placement changed only by editing the table") named a person's
decision as the fault instead of naming the missing mechanism.

### 12 · Where it broke — `s12_where_it_broke.py` — **explanation**

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

### 13 · What golem is, and is not — `s13_what_golem_is.py` — *before / after split* — **explanation**

**Not:** a replacement for bare-metal provisioning, OS installation, or the
basics of networking and security. **Is:** a replacement for the custom Python
and the new Ansible being built in December and January. A bar between them:
layer 1 stays where it is. Closing line, and the definition of *declarative* the
deck has been circling: you write the state a host should be in, and golemd works
out the steps.

### 14 · What you need, and what meets it — `s14_requirement_and_property.py` — **explanation**

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

### 15 · The pipeline — `s15_the_pipeline.py` — *swimlane pipeline* — **explanation**

Five stages: `fleet.emet` → `emetc build` → **manifest** → `golemctl apply` →
**golemd** on the host. Subtitle: each host diffs its own scroll from one
manifest. Below the stages the manifest, quoted: `Manifest { format_version,
emet_version, scrolls: Vec<AddressedScroll> }` with `FORMAT_VERSION = 5`,
`AddressedScroll { content_id, scroll }`, and `ContentId` as a 32-byte BLAKE3
digest over postcard bytes, one per scroll and one per glyph. All verified against
`libs/scroll-format/src/manifest.rs` and `content_id.rs`.

The closing bar ("Same content id, no work.") moved to slide 16, where the ops it
refers to are actually drawn.

### 16 · Inside golemd: the diff — `s16_the_diff.py` — *before / after split* — **explanation**

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

### 17 · Inside golemd: apply and undo — `s17_apply_and_undo.py` — *loop* — **explanation**

Three cards and a return arrow that closes the loop.
`Reconciler::apply(&Glyph, ContentId) -> EnactResult<Outcome>` with `Outcome { op,
cid, inverse, changed }` — apply captures the prior state as an `Inverse`, carried
on the `Outcome`. Then `Revision { id, created_at, kind, scroll_content_id,
outcomes }` with `kind: RevisionKind = Init | Reconcile`, the append-only journal
of what golem applied. Then `Reconciler::reverse(&Outcome) -> EnactResult<()>`,
which replays that `Outcome` to restore the prior state exactly. Closing bar:
golem reverses only the edits it recorded.

Both signatures are fallible and were drawn as though they were not
(`apps/golemd/src/reconciler.rs:46-47`). `scroll_content_id` is an
`Option<ContentId>`, `None` on an `Init` revision.

### 18 · One program, one scroll per host — `s18_the_scroll_tree.py` — *tree* — **explanation**

`main : List Scroll`, then a Scroll forking into a **branch** (named sub-scrolls)
or a **leaf unit** (glyphs, and an optional policy). Line under it: either glyphs
or named sub-scrolls at each level — never both. Two callouts: a leaf unit is the
failure-isolation boundary, one unit's failure never rolls back a sibling; and
workloads, quadlets and ingress are Emet libraries that compile down to the four
glyphs.

### 19 · The four glyphs — `s19_the_four_glyphs.py` — **reference**

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

### 20 · golemctl — on your machine — `s20_golemctl_verbs.py` — **reference**

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

### 21 · golemd — on the host — `s21_golemd_routes.py` — **reference**

The eight registered routes, each glossed: `POST /manifest`, `POST /plan`,
`GET /reconciles/latest`, `GET /reconciles/:id`, `GET /state`, `GET /revisions`,
`GET /revisions/:id`, `GET /status`. `against_host` and `after` are optional query
parameters, named in the glosses rather than baked into the paths, because the
paths registered in `apps/golemd/src/http.rs:54-62` are bare.

Then the conflict codes as three badges: `409 HostBusy` (a host-reading plan met
an apply in flight), `409 ReconcileInProgress` (an apply met an apply), and
**no conflict** (a plan that does not read the host never blocks). The third badge
used to read "plan still works", a clause where the other two were outcomes.

### 22 · Plan before apply — `s22_plan_against_host.py` — **explanation**

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
network and one set of library versions with every other process; a container is
still a process on that host, with an image, namespaces and cgroups added; an
image is read-only layers plus a config, named by its digest; the registry is the
only thing a host has to reach; and the container runtime does all of this on one
machine and none of it across machines.

**06 is the hinge.** Many hosts means a control plane, workers and a
desired-state store — you name what should run, and the control plane chooses
where.

**07 names the five jobs**, using the same five names as slide 05 of the golem
deck, because they are the same five jobs. 08 through 15 then take four of them
one at a time: placement (08, 09), lifecycle (10), health and reconciliation
(11), and supporting plumbing spread across connectivity (12, 13), scaling (14)
and storage and secrets (15).

**08 is the slide the deck exists for.** Placement is the only part that chooses
a node, and it is drawn as an act: an unplaced workload, the candidate nodes, and
the binding that settled it. The definition comes first — a binding is the record
that this workload runs on that node — and the only-part claim follows it as the
closing line.

**16 and 17 land it.** 16 is one matrix across Docker, Swarm, Nomad and
Kubernetes — who provides which piece. 17 says where golem sits: declarative
desired state plus reversible enactment, and deliberately **not** a scheduler.

## The seventeen slides

| # | Title | Module | Form | Mode |
|---|---|---|---|---|
| 01 | A process on a host | `s01_a_process_on_a_host.py` | split / cards | explanation |
| 02 | What a container adds | `s02_what_a_container_adds.py` | icon cards | explanation |
| 03 | The image | `s03_the_image.py` | hub / stack | explanation |
| 04 | Registry, pull, run | `s04_registry_pull_run.py` | icon cards, flow | explanation |
| 05 | One host, many containers | `s05_one_host_many_containers.py` | host with workloads | explanation |
| 06 | Many hosts: the cluster | `s06_many_hosts_the_cluster.py` | cluster map | explanation |
| 07 | The five jobs | `s07_the_five_jobs.py` | numbered stack | reference |
| 08 | Placement: the binding | `s08_placement_the_binding.py` | binding mark | explanation |
| 09 | What the scheduler weighs | `s09_placement_what_it_weighs.py` | hub / cards | explanation |
| 10 | Lifecycle | `s10_lifecycle.py` | state machine | reference |
| 11 | Health and reconciliation | `s11_health_and_reconciliation.py` | loop | explanation |
| 12 | Connectivity: addressing | `s12_connectivity_addressing.py` | before / after split | explanation |
| 13 | Connectivity: the service | `s13_connectivity_the_service.py` | icon cards, flow | explanation |
| 14 | Scaling | `s14_scaling.py` | replica set | explanation |
| 15 | Storage and secrets | `s15_storage_and_secrets.py` | split | explanation |
| 16 | Who provides which piece | `s16_who_provides_which_piece.py` | matrix | reference |
| 17 | Where golem sits | `s17_where_golem_sits.py` | before / after split | explanation |

**Each of 01 to 06 and 13 opens on a definition, because each introduces a term.**
A process is a running program sharing one machine with every other program. A
container is a process on the host, given three things it did not have. An image
is read-only filesystem layers plus a config, named by the digest of its contents.
A registry is a server that stores images and serves them by digest. A container
runtime is the program that runs containers on one host. A cluster is many hosts,
a store of the state you want, and a control plane. A service is one stable name
for a changing set of instances. Those seven lines are the deck's whole vocabulary
teaching, and each replaced an evocation — "Layered, content-addressed, and never
edited in place", "Desired against actual, forever", "Two ways to wire it".

Slide 16's shape, and why it is worth drawing: Docker on one host leaves almost
everything to you; Swarm and Kubernetes provide almost all of it; Nomad provides
most of it but leaves supporting plumbing and secrets to Consul and Vault. That
asymmetry is the information.

Slide 17 makes exactly two claims, and no more. golem is declarative desired
state and reversible enactment — every edit records its inverse, so a change can
be taken back exactly. golem is not a scheduler: nothing in it chooses a node, and
the program names the host the way a person would. Its closing line — placement
stays a decision a person makes, written down and versioned — is the deck's
position on manual placement, and slides 10 and 11 of the golem deck are written
to agree with it. If the two ever disagree again, this is the one that is right.

**"Answers" is not a verb for a tool.** A platform provides, a person decides, a
runtime does. Legends across both decks read *provided by the platform* / *you
provide it*, and the same discipline retires "a playbook reached it" and "the
image carries it".

---

## Corrections against the code

Earlier drafts got these wrong. All are drawn correctly now; they are recorded so
nobody reintroduces the error. Each was found by reading the definition, not by
paraphrasing the previous slide — which is how several of them survived as long as
they did.

**`reconcile::plan` does not take two scrolls.** `plan(prior: &[Outcome], desired:
&Scroll) -> Vec<GlyphOp>` (`apps/golemd/src/reconcile.rs:23`). Slide 16 drew both
panels as `AddressedScroll { content_id, scroll }`; `prior` is the journalled
outcome list.

**`apply` and `reverse` are fallible.** `apply(&Glyph, ContentId) ->
EnactResult<Outcome>` and `reverse(&Outcome) -> EnactResult<()>`
(`apps/golemd/src/reconciler.rs:46-47`). Slide 17 drew both as infallible.

**There is no drain, and no cross-host ordering.** No drain operation exists in
`apps/` or `libs/`. `golemctl fleet` spawns one task per target and joins them
afterwards, with no barrier, dependency edge or concurrency limit
(`apps/golemctl/src/fleet.rs`); a failure on one host neither stops nor rolls back
another. Within one host, `plan` orders installs and replaces first and removes
last (`reconcile.rs:20-22`). Slide 14 claimed "so drain is real"; slide 11 must
keep saying that nothing orders host A before host B.

**golemd puts no agent-free binary on the host — golemd *is* the agent.** Slide
14's "no interpreter, no runtime, no agent" was self-contradicting.

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
its subtitle is only "An Emet program is typed and functional, and evaluates to a
list of scrolls" — so nothing false is drawn. The question is still open, and
needs an answer
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
