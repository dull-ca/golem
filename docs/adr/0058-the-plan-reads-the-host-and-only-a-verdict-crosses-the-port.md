# 0058 — The plan reads the host, and only a verdict crosses the port

## Status

Accepted 2026-08-11 (decision by Dr. Dub). Extends
[ADR 0036](0036-plan-verb-and-unit-notifies-reload.md), which built the plan
verb against the journal: the plan gains a second, opt-in comparison basis, and
0036's `POST /plan` contract, its exit code, and its collapsed rendering are
otherwise unchanged. 0036's rejection of a client-side plan is inherited rather
than reopened, and it reaches further here than it did there — the diff merely
*needed* the journal, which lives with golemd, while host state is not
inconvenient for `golemctl` to reach but unreachable: no client can read a
remote host's dpkg database, `/etc`, or systemd. Extends
[ADR 0015](0015-reversible-reconcilers-and-content-addressed-versioning.md) §5,
whose words are "every `apply` first observes actual host state". That
observation already exists in golem and is reachable only by writing; this names
it as a port method and gives it a read-only caller. Leaves
[ADR 0014](0014-golemd-glyph-rewrite-and-model-reconciliation.md) §3 unamended —
the reconcile planner stays pure, takes no new parameter, and gains no sibling —
and nothing below is a concession on that boundary. Closes the open question in
[ADR 0057](0057-clearing-a-latched-failure-before-starting-a-unit.md)'s
Consequences, "whether golem should look for latched units outside a diff-driven
op is open, and is a question about the diff, not about this verb", in the
affirmative and **for reporting only**. Argues its privilege boundary against
[ADR 0047](0047-typed-secrets-on-the-wire.md)'s sealed `Text` and
[ADR 0042](0042-ssh-transport-loopback-bind-shared-secret-auth.md)'s loopback
bind and bearer token, and preserves
[ADR 0030](0030-apt-package-version-and-index-policy.md)'s rule that a `Latest`
apt glyph never participates in drift detection. Implementation lands on
`lakin/plan-live-host-diff`: a new `apps/golemd/src/observe.rs`, the port method
in `reconciler.rs`, the per-kind probes in `reconcilers.rs`, the wire fields in
`plan_report.rs`, the second block in `apps/golemctl/src/plan.rs`, and the
probe's serialization against a live apply in `foreman.rs`
(`write_activity_snapshot`, `try_write_guard`, `ForemanError::HostBusy`).

## Context

`golemctl plan` answers "what will this apply do?" out of the journal alone —
prior outcomes against the desired scroll, diffed by content id. The journal is
golem's record of what golem enacted. It is silent about everything else on the
host, and two cases turn that silence into a wrong answer.

**Enrollment.** Pointing golem at a host another tool already configured gives
an empty journal, so every glyph is an `Install` and the plan reads *17
changes*. The host may already hold all seventeen byte for byte. The operator
cannot tell "this rewrites the box" from "this changes nothing" — the question
being asked before the first apply of a fleet's life.

**Drift.** A glyph whose content id has not moved produces a `Noop`, and a
`Noop` enters no reconciler. If someone hand-edited the file, or a unit is
sitting inactive, the plan says there is nothing to do: right about the journal,
wrong about the host. ADR 0057 hit the same hole from the apply side and left
the diff-side half open on purpose.

The observation needed to answer both already exists. Every apply function opens
by asking whether the host already realizes the glyph and returns early if it
does (six sites in `reconcilers.rs`); ADR 0015 §5 states it as a rule. The
predicate is written, tested, and load-bearing, and the only way to reach it is
to run an apply. What is missing is a read-only caller, not a new answer.

Four constraints shape how far that answer may travel. `perms_match`
(`reconcilers.rs:869`) resolves `owner: Some("nginx")` to a uid through the
host's own `/etc/passwd`, so whether ownership matches is not a function of the
glyph and the file alone. ADR 0047 puts the keyring inside the reconciler —
`HostReconciler.keyring`, which the foreman never sees — and `golemctl` does not
even link `scroll-format`'s `secrets` feature. ADR 0014 §3 makes the diff a pure
fold over two content-addressed glyph sets, and nothing about a live host
belongs in it. And `POST /plan` is already loopback-bound and
bearer-authenticated (ADR 0042), reachable only through an operator's SSH
tunnel.

## Decision

`golemctl plan` and `golemctl fleet plan` gain `--against-host`. With the flag,
golemd probes the host for every glyph the plan already names and renders a
second block beside the journal one: what the journal says will happen, and what
the host says is already true.

**The reality diff is read-only and never changes what an apply does.** No
`Observation` reaches `run_reconcile`. No op is added, dropped, reordered, or
reclassified by one, and nothing about it is journaled. A plan whose host block
reads `= match` on every glyph still applies every glyph — the apply's own early
returns, the same predicate in the same code, are what make that cheap. Feeding
an observation into enactment would have golem act on state it never recorded,
which is the ADR 0015 rule the journal rests on.

- **A verdict crosses the `Reconciler` port, never host state.**
  `observe(&[GlyphOp]) -> Observations` asks the domain's question — does the
  host already realize this glyph? — and the adapter answers with a four-valued
  `Observation` (`Realized` | `Divergent` | `Absent` | `Unknown(Unknowable)`),
  not with `(contents, mode, uid, dpkg status)` for a pure core to compare.
  Three reasons, in order of weight. The comparison cannot be pure: it resolves
  usernames against the host's passwd database, which a pure comparator would
  have to be handed. It needs the fleet key, and if raw state crossed the port
  then `POST /plan` would return the plaintext of the host's secret-bearing
  files to a client that cannot even represent a sealed `Text` — an
  exfiltration endpoint wearing a plan's name. And a second implementation of
  "is the host already right" is a second source of truth for the one predicate
  whose disagreement makes the plan lie.
- **`reconcile::plan` is untouched, and the reality column is not a second
  `Vec<GlyphOp>`.** The I/O is hoisted out: probe first, producing
  `Observations` — a plain `BTreeMap<String, Observation>` — which the report
  builder consumes as data. `Foreman::plan_manifest` keeps its signature and
  delegates to `plan_manifest_scoped(bytes, PlanScope::JournalOnly |
  JournalAndHost)`; an enum, not a `bool`, because `(bytes, true)` is unreadable
  at the call site. Reusing `GlyphOp` for the host column would need a synthetic
  `old_cid`, and content-id equality is the wrong equality: a secret-bearing
  file whose plaintext is identical hashes differently
  (`Text::Composed([Hole])` against plaintext bytes), so it would report a
  spurious `Replace` precisely where secrets are involved.
- **Lookup is total, the port method is defaulted, and the probe cannot fail.**
  `Observations::get` returns an `Observation`, never an `Option`; a key the
  probe never reached reads `Unknown(NotModelled)`, so a partial probe degrades
  to stated ignorance rather than a missing row or a panic. The trait default
  answers an empty map, which is the same thing, so all 32 `impl Reconciler` in
  the tree compile untouched — but the blanket forwarders for `Arc<R>` and
  `Box<R>` and the `PanicCatching` wrapper must each forward `observe`, because
  `Foreman.reconciler` is a `Box<dyn Reconciler>` and a missing forwarder
  silently no-ops the feature in production with every test still green.
  `observe` is infallible by contract, like `diagnose` and unlike `apply`: a
  plan that 500s over one unreadable file is worse than one unknown row.
- **A `Remove` is asked the weaker question, and the renderer phrases the
  answer.** The host cannot say whether golem once owned a resource, which is
  why the reality column never *originates* a `Remove`. Given a `Remove` the
  journal originated, the host answers whether the resource is still there —
  the out-of-band signal the feature exists for, already in hand. `Observation`
  stays glyph-relative on the wire and the inversion lives in the renderer,
  which knows the action: `Absent` renders `= gone` under a `Remove` and
  `≠ missing` under an `Install`. Presence needs no key, so removes never reach
  the sealed case at all.
- **The host block covers every desired glyph, `Noop`s included — which closes
  ADR 0057's open question, for reporting.** A `systemdService` glyph whose
  content id has not moved enters no reconciler, so nothing in golem today asks
  whether the unit is running. Under `--against-host` it is asked: a unit that
  is not both enabled and active reports `Divergent` even where the journal says
  `Noop`. That row is the most valuable one the feature produces — golem thinks
  this is settled, the host says it was edited or it fell over — and suppressing
  it to keep the two blocks row-for-row parallel would gut the point. It stays a
  report: nothing here starts a unit, clears a latch, or widens the set of units
  ADR 0057 gave the forcing verb.
- **`--against-host` exists for cost and surprise, not as a security control,
  and this is stated rather than left to be inferred.** `observe` uses the same
  keyring, in the same process, on the same host, as `apply_streaming`. It reads
  exactly the paths a subsequent apply would read, with authority golemd already
  holds, of files the manifest already names — a change in behavior, not in
  authority. A caller who can plan can already apply, which is strictly more
  powerful. What keeps plaintext off the wire is the port's shape, not the flag;
  the standing invariant is that **no reality-diff wire field ever carries host
  state, only a verdict**.
- **A host that cannot open the manifest's secrets says so per glyph.**
  `observe` calls `keyring.open`, declines to propagate the `Fatal`, and records
  `Unknown(Sealed)` — a gain, because the plan now surfaces before anything is
  touched that this host cannot enact the manifest's secrets. Per-glyph, never
  fatal to the plan; it names the reason rather than printing a bare `?`; and it
  does not move the exit code, because a diff is not an error and an unknown is
  not a diff.
- **Batch apt in v1, on measured behavior rather than assumed.** One
  `dpkg-query -W -f='${Package} ${Status}\n' …` for the whole scroll replaces
  one subprocess per package, and `Observations` being a map dedupes a package
  declared in three units for free. Three facts fix the parser, each verified by
  running the command on a Debian trixie guest rather than reasoned about.
  `dpkg-query` writes a record to stdout for every name it knows and exits 1
  when any name is unknown, putting the complaint on stderr — so the batch reads
  stdout regardless of exit status and ignores stderr, and `apt_installed`'s
  `query.succeeded()` gate (`reconcilers.rs:211`) is wrong for it. A package apt
  knows but that was never installed has no record at all, so a name absent from
  stdout is `Absent`; a removed-but-configured one reads `deinstall ok
  config-files`, which the exact match on `install ok installed` already
  excludes. And the field is `${Package}`, not `${binary:Package}`: the latter
  appends an architecture qualifier to every `Multi-Arch: same` package —
  several hundred on a stock trixie box, `libc6:amd64` among them — so a glyph
  naming `libc6` would have observed `Absent` on a host that plainly has it.
  `${Package}` emits no colon anywhere in the dpkg database, and picking the
  field that never produces a qualifier beats stripping one off. That near-miss
  is this ADR's own thesis turned on itself — a probe that is subtly wrong makes
  the plan lie about exactly what the column was added to check — and no amount
  of reasoning about `dpkg-query` would have caught it. A test over recorded
  stdout pins all three so the parser cannot regress.
- **Leave systemd per-unit.** The `is-enabled` + `is-active` pair buys exact
  predicate identity with `apply_systemd` for ~2N cheap spawns. `systemctl show
  --property=UnitFileState,ActiveState,LoadState` would collapse it, but
  `UnitFileState`'s value set is not exit-code-identical to `is-enabled`'s —
  `static`, `indirect`, `generated`, and `linked` all exit 0 — and a wrong
  allowlist there makes the plan lie about a running service. Unlike the apt
  batch, nobody has run it: that mapping is unverified, and the deferral hedges
  the unknown rather than preferring the spawns. `docs/TODO.md` carries it,
  gated on an equivalence test.
- **The fake reconciler must be able to make the two columns disagree.**
  `FakeReconciler`'s `present` map is golem's own record, so an `observe` that
  read it naively would always agree — and a fake that cannot produce
  disagreement gives the join, summary, and render path, where this feature's
  whole risk lives, a harness that only exercises the happy path. It gains
  `preexisting(key, cid)` and `vanished(key)`, and honors the keyring through
  the existing `openable`, which is the only way the keyless-host case above is
  tested against the default reconciler.
- **With the flag off, the output is byte-identical to today.** No block labels,
  no host rows, and `observed` omitted from the JSON entirely, so a client can
  distinguish "not asked" from "asked, and it matched" and the plan goldens are
  untouched. The one exception is a footer line shown only when the host has
  committed nothing yet *and* the flag was not passed — `no prior revision
  here · --against-host checks what this host already has` — because
  enrollment is where the flag is worth knowing about and the only place a
  hint costs nothing. "Committed nothing yet" is `against_revision.is_none()
  || against_revision == Some(1)`, not `is_none()` alone:
  `wal::latest_revision_id` returns `Some(1 + committed_count)` and never
  `None` once a host has a WAL, so a bare `is_none()` check would be dead code
  on every real host. Revision 1 is the `Init` row itself — golem has enacted
  nothing — which is exactly the enrollment moment the hint exists for.
- **`host_already_matches` is computed server-side:** true when every desired
  glyph is `Realized`, every `Remove`'s resource is already gone, and
  **nothing** is `Unknown`. That last clause is the one a client would get
  wrong, and getting it wrong means calling a host safe golem could not check.
- **A host probe is checked against golem's own write activity by snapshot
  and recheck, never by holding a lock across it.** Only the `JournalAndHost`
  arm is gated — `JournalOnly` reads the append-only, monotonic WAL and never
  touches the write lock, so it keeps succeeding during an apply exactly as
  it always has. `Foreman::observe_against_host` takes a
  `write_activity_snapshot` before calling `Reconciler::observe`, calls
  `observe` with **no lock held at all**, then takes a second snapshot after
  and compares. `write_activity_snapshot` itself calls `try_write_guard`, a
  non-blocking sibling of `write_guard`: `None` means an apply already holds
  the lock, mid-reconcile, and answers `HostBusy` immediately. A free lock is
  not by itself proof nothing is running — `ingest` opens an attempt and
  marks it `Enacting` under one write-lock acquisition, releases the lock,
  and `run_reconcile` re-acquires it separately — so between those two
  acquisitions an attempt can be live while the lock sits free. The same
  unsettled-attempt check `ingest`'s own gate makes closes that second
  window — made while `write_activity_snapshot` still holds the guard,
  which it then drops before `observe` runs. Two snapshots that agree —
  same `(reconcile_id, phase)`, or both `None` — mean the probe ran clean;
  a mismatch, or either snapshot alone answering `HostBusy`, returns
  `ForemanError::HostBusy`, mapped to `409` and distinct from
  `ReconcileInProgress` because that error's advice — poll the running
  attempt instead of re-applying — answers a different question than the one
  a plan asked. Comparing the two snapshots, rather than holding one lock
  across the whole probe, is what catches an apply that both starts and
  finishes entirely inside the probe's window: invisible to either snapshot
  alone, visible the moment they are compared.
- **That guarantee covers the probe alone, and the response's other fields
  say less than "refused, not raced" would imply.** `plan_manifest_scoped`
  reads `against_revision` and the WAL steps behind the journal column
  before the host probe starts, unsynchronized with any concurrent write.
  A reconcile that commits between that read and the snapshot-and-recheck
  around `observe` produces a report whose journal column reflects the host
  before the apply and whose host column reflects it after — two columns
  from two different instants in one response. This is judged harmless
  rather than closed: a stale `prior` can only over-produce ops (a glyph the
  just-finished apply already wrote still looks pending on the journal
  side), and an over-produced op's host verdict reads `Realized` regardless,
  so the extra row corrects itself in the reader's eyes rather than
  misleading them. But "refused, not raced or queued" describes the probe's
  own guarantee against golem's writer, not the response as a whole.
- **Refuse rather than block, on both a practical and a deeper argument.**
  `run_reconcile` holds the write lock for an entire reconcile — apt installs
  plus the whole retry budget, and `TUTORIAL-fleet.md` documents a cold
  guest's first apply as "many minutes" — and `golemctl` builds its HTTP
  client with no timeout configured, so a queued plan would not time out: it
  would hang the operator's terminal for however long the apply ran, and
  stall `fleet plan`'s concurrent fan-out on that one host while every other
  host finished. But the stronger reason is not the missing client timeout:
  an apply in flight means the host is being actively rewritten, so a reality
  diff taken then is not a stale-but-valid snapshot, it is a reading of a
  moving target. Refusing is more truthful than handing back an
  authoritative-looking answer measured at an arbitrary instant mid-rewrite.
  The cost is real and stated plainly: the operator has to retry, and on a
  long apply may retry more than once — the price of not lying.
- **Deadlock and starvation were checked, not assumed.** `observe` takes no
  golemd-level lock at all — not `apt`, not `daemon_reload`, not
  `line_locks` (`HostReconciler::observe`/`observe_op`/`observe_apt_batch`
  call only `self.runner.run` and `self.keyring.open`) — so there is no
  second lock for the write guard to order against, and deadlock is
  structurally impossible. Starvation is now moot in both directions:
  `try_lock` never queues, so a plan can never starve an apply waiting for
  the lock, and since the guard is never held across `observe`, a plan
  cannot delay a waiting apply either — the guard it does take is released
  before `observe` runs, so there is no window in which a probe holds
  anything an apply needs.

## Consequences

- **Enrolling a host another tool already configured is answerable before
  touching it.** The driving case collapses to one line — every declared glyph
  already matches, applying this manifest changes nothing — a statement no
  amount of reading the journal could have produced.
- **The reality column is only as sharp as what golem models, and the gaps are
  per kind.** `lineInFile` has no `Divergent` at all: the line is among the
  others or it is not. `aptPackage` has none today either, being presence-only;
  if ADR 0030's `Pinned(version)` ever ships, a wrong version becomes the first
  real apt `Divergent`, and a `Latest` glyph must stay presence-only so the
  probe never touches the apt index. `systemdService` is `Realized` only when
  enabled and active, `Absent` when `is-enabled` reports the unit unknown, and
  `Divergent` otherwise. Under `--reconciler fake`, `observe` reports the same
  `Realized`/`Absent`/`Divergent` verdicts as the real reconciler, read off its
  own `present` map, and `Unknown(Sealed)` only when the keyring cannot open
  the glyph — never as a default. A fake that agreed with itself on every row
  by default would hide the join/summary/render risk this feature actually
  carries, which is why it gained `preexisting`/`vanished` (Decision, above):
  the only way to make the two columns disagree under the fake.
- **A glyph naming no `owner` reports `Realized` whatever the owner is.**
  `perms_match` reads `owner: None` as "leave as-is", so the column agrees with
  what an apply would do and disagrees with what a reader may expect it to mean.
- **A `dpkg-query` failure takes out every apt glyph in the scroll at once,
  not one.** `observe_apt_batch` (`reconcilers.rs`) is one call for the whole
  scroll, which is what makes the probe cheap enough to run before every
  plan; the accepted cost is that a spawn failure, or a dpkg exit status of 2
  or more (dpkg's own signal for a locked or corrupt database), degrades
  every apt glyph together to `Unknown(Unreadable)`, while exit 1 — some
  requested name unknown to dpkg, every known name still printed — stays
  parseable per package. `Unknown` rather than a confident `Absent` is the
  point of the trade: a fatal exit parsed as "these packages are not
  installed" would have reported every apt glyph `≠ missing` with full
  confidence, which is the exact direction this ADR exists to keep out of
  the reality column, and the direction the exit-2 case had wrong before it
  was caught. A test pins the spawn-failure half of the blast radius
  (`a_dpkg_query_spawn_failure_takes_out_every_apt_glyph_but_leaves_the_rest_of_the_scroll_alone`,
  `reconcilers.rs`).
- **The probe is serial and uncached, by choice.** After the apt batch a scroll
  costs one subprocess plus roughly two per unit, plus a few hundred syscalls; a
  worker pool buys nothing measurable and makes failure modes nondeterministic.
  A cache would make the plan lie about "right now", the one thing it sells.
- **The probe now fails closed rather than races during an apply, and
  retrying is the cost.** `--against-host` answers `409` (`host-busy`) when
  a `write_activity_snapshot` taken before or after `observe` finds the
  write lock held, finds it free but the latest attempt unsettled, or finds
  the two snapshots disagree — the reconcile id or phase moved during the
  probe itself (Decision, above); a journal-only `golemctl plan` is
  unaffected and always succeeds, apply or no apply. The operator retries,
  and on a long apply — apt installs plus the full retry budget can run
  minutes — may have to retry more than once.
- **Two concurrent `--against-host` plans no longer exclude each other.**
  Neither ever holds the write lock across `observe`, so each only contends
  with the other for the instant it takes to read the latest attempt, not
  for the probe's whole duration. The discarded first mechanism — hold
  `try_write_guard` across `observe` — made the second plan 409 with a
  message asserting an apply was running, which was false: a read excluding
  a read, not a read excluding a write. That is gone; a read no longer
  excludes a read.
- **The `HostBusy` message states only what golem knows, not that a specific
  apply is running now.** golem knows it was writing to the host somewhere
  inside the read window; it does not know whether that write is still in
  flight, already finished, or an attempt wedged `Enacting` with no active
  writer at all. `message()` claims exactly the first and none of the rest:
  "golem was writing to this host during the read, so the read could not be
  trusted as a snapshot." — silent on whether an apply is running *now*,
  which the discarded mechanism's wording had implied and which is not
  always true.
- **A wedged probe now fails closed against its own request, not the
  daemon.** `observe`'s subprocess runner carries no timeout, so a stalled
  mount or a hung systemd can still hang the `--against-host` call that hit
  it. What changed is the blast radius: since no lock is held across
  `observe`, a wedged probe no longer wedges `write_guard` for every other
  request — `POST /manifest` and every other plan keep working while it
  hangs. A subprocess timeout on `observe` remains open future hardening,
  not something this rework provides.
- **`fleet plan --against-host` can now fail a fleet-wide command over an
  ordinary rolling-apply state.** `run_plan`
  (`apps/golemctl/src/fleet.rs`) maps any per-host error — `HostBusy`
  included — to `HostPlan::Error` and exits `1` if any host lands there,
  which sits against `plan`'s own documented "always exits 0": a `409`
  while one host in a fleet is mid-apply is ordinary, not exceptional, yet
  it now fails the whole `fleet plan --against-host` invocation. This is a
  deliberate call — a refusal is not a diff, and folding it silently into an
  unchanged exit code would hide that the reading could not be trusted — but
  it is a real behavior change for anyone scripting `fleet apply` followed
  by `fleet plan --against-host`.
- **Serializing against golem's own writer is not a stable-snapshot
  guarantee.** The snapshot-and-recheck closes exactly one window: golem
  racing itself. It says nothing about the host changing out of band between
  the second snapshot inside `observe_against_host` and the operator reading
  the printed report — and out-of-band change is the entire reason
  `--against-host` exists. A host plan that returns without a `409` is a
  truthful reading taken at one instant, not a promise that the instant
  still holds by the time it is read. A reader who takes a successful run as
  "the plan I am looking at is still true" has misunderstood it.
- **Two residual leaks, named rather than denied.** A `Divergent` on a
  secret-bearing file tells the caller one bit — the host's copy differs from
  the one they hold — and they already hold the desired plaintext and the power
  to overwrite it. And `observe` is a path-existence oracle for paths the
  manifest itself names. Neither is worth a gate.
- **What a `Divergent` differs *in* is deferred, not undecided.** A closed facet
  enum (`Contents | Mode | Owner | Group | Target`) is safe, useful for triage,
  and the obvious second version; anything resembling a value or a diff is a
  leak. Named here so it is not reinvented as a free-text field. The
  motivation reaches further than triage: `observe` already reports
  `Divergent` in two cases where `apply` would hard-fail instead — a
  `directory` glyph finding a pre-existing regular file at its path
  (`apply_directory`'s "refuse to replace pre-existing non-directory … with a
  directory") and a `symlink` glyph finding a pre-existing symlink pointing
  elsewhere (`apply_symlink`'s "refuse to repoint pre-existing symlink … ").
  `Divergent` is not wrong there, but the probe already knows the apply will
  hard-fail, and a `Divergent(WrongKind)` facet would let the plan say so
  before anything runs — this feature's own thesis, that the host's answer
  beats silence, applied one level deeper.
- **Two questions are left to measurement rather than argument.** Whether
  reading whole files to compare them is affordable depends on real scroll file
  sizes, which nobody has measured; the `metadata.len()` early-out catches the
  easy case, and if a fleet ships multi-MB files the answer would be a content
  hash — which changes the predicate and reopens the one-source-of-truth
  argument above. Collapsing every `= match` into a single row is right for the
  enrollment and drift cases and may be wrong for a mid-size scroll; it is
  render-only.
- **`host_already_matches` is the field most likely to draw pushback.** It is a
  server-side judgement in a report that is otherwise data, and it is there
  because its unknown-is-not-agreement rule is the easy one to get wrong. If it
  goes, that rule has to move somewhere every client obeys.
- **The documentation gate (ADR 0054) reaches further than the flag.** The CLI
  reference, the `/plan` endpoint row, the "read the diff first" page, and
  `explanation/trust.mdx` — already the promise that secrets stay reviewable in
  `golemctl plan`, and so the home for verdict-never-plaintext — all move, and
  the two tutorial fences showing a default plan against no prior revision must
  be checked against the new footer line. `apps/fleet/` needs one argv
  passthrough; it parses no plan JSON.

## Alternatives considered

- **Ship host state across the port and compare it in the pure core.**
  Rejected: the comparison resolves usernames against the host's passwd database
  and unseals secrets with a key ADR 0047 keeps inside the adapter, so a pure
  comparator needs both shipped to it — and the version of that which reaches
  `golemctl` hands the caller the plaintext of the host's current secrets.
- **Model the host as a second `Vec<GlyphOp>` and reuse the existing diff.**
  Rejected: it needs a synthetic `old_cid`, and content-id equality reports a
  `Replace` for a secret-bearing file whose plaintext already matches — a wrong
  answer in exactly the case that matters most.
- **Compute the reality diff in `golemctl`.** Rejected on ADR 0036's ground,
  which is stronger here: the client cannot see the host at all.
- **Add a `host: Option<&Observations>` parameter to `reconcile::plan`.**
  Rejected: it buys nothing the report builder cannot do with the same data, and
  spends ADR 0014 §3's boundary to buy it.
- **Gate `--against-host` behind a new authorization scope.** Rejected: it
  raises no privilege, and a gate would advertise a boundary that is not there.
- **Name the flag `--drift`.** Rejected: drift is one of the two answers, not
  the question, and the driving case — never applied, already identical — is the
  opposite of drift. `--against-host` also parallels the headline the plan
  already prints, which is always "against" something.
- **Add an `unmanaged` verdict for a resource the host holds that golem did not
  put there.** Rejected: ownership is precisely what the host cannot report, and
  naming a state after the unknowable invites everyone to read it as known.
- **Report `unknown` for a journal `Remove`, or omit the row.** Rejected:
  presence is answerable, the probe is already running, and `unknown` stays
  meaningful only while it is reserved for genuine ignorance. Omitting the row
  loses the "already gone" signal and breaks the block's alignment.
- **Batch systemd with `systemctl show` in v1.** Rejected for now on an
  unverified premise rather than a settled one: `UnitFileState` may or may not
  partition the way `is-enabled`'s exit code does, and a plan that misreports a
  running service is worse than 2N spawns. Reopen it with the equivalence test,
  not without.
- **Exit nonzero when the host disagrees.** Rejected: `plan` exits 0 whether or
  not changes exist (ADR 0036), and a second column is no reason to change what
  the first one's exit code means.
- **Block instead of returning 409.** Rejected: `run_reconcile` holds the
  write lock for a whole reconcile, and `golemctl`'s HTTP client carries no
  configured timeout, so a blocking wait would not time out — it would hang
  the operator's terminal for however long the apply took, and stall `fleet
  plan`'s concurrent fan-out on that one host while every other host
  finished. The absence of a client timeout argues against blocking, not for
  it.
- **Hold `try_write_guard` across `observe`.** Built first, then discarded.
  `observe` shells out roughly `1 + 2N` subprocesses — one `dpkg-query`,
  then `systemctl is-enabled`/`is-active` per unit — through a runner with
  no subprocess timeout, so holding golemd's one write lock across that call
  means a wedged systemd, a stalled mount, or a D-state child stops hanging
  one HTTP request and starts hanging the whole daemon: every later `POST
  /manifest` blocks forever inside `write_guard`, `recover()` included, and
  `golemctl` sets no client timeout to notice with. The bound this version
  claimed — a waiting apply delayed by at most one probe — was not a bound
  at all, since a wedged probe never finishes; a read must not be able to
  block a write. Replaced by the snapshot-before, `observe`-unlocked,
  snapshot-after, recheck mechanism in the Decision above, which never holds
  the lock across `observe`.
