# 0033-async-apply-with-live-progress

## Status

Proposed 2026-07-26.

Supersedes **in part** ADR 0029 (best-effort reconcile and structured failure
reporting): the `ReconcileReport` shape, the tree-of-units structure, and the
in-band-failure philosophy (a partial or rolled-back reconcile is a *result*, not
a transport error) all stand — this ADR changes only the **transport** that
carries them. ADR 0029 §5's "HTTP 200 with the `ReconcileReport` body,
synchronously, on the same request that enacted" is what is superseded; the report
now arrives on a *second* request. Builds on ADR 0020 (the write-ahead log — the
progress this ADR streams is a projection of it) and ADR 0031 §6 (the WAL's
`unit_path`, which shapes the projection into per-unit rows). Works in step with
ADR 0013 (fleet and golemd ship in lockstep — no dual-protocol window). None of
0020, 0031, or 0013 is superseded. Mark ADR 0029 "Superseded in part by 0033" in
its status.

## Context

`POST /manifest` is **synchronous today** (`http.rs:66`). The axum handler
`spawn_blocking`s `foreman.apply_manifest`, which runs the *entire* reconcile —
recovery, planning, best-effort enact with retry rounds, config propagation,
settle — and only then returns the `ReconcileReport` as an HTTP 200 body
(`foreman.rs:189`, `:228`). The whole reconcile happens inside one held-open HTTP
request. A reconcile is not short: a cold host runs `apt update`, package
installs, image pulls inside `systemctl start`, and up to `max_attempts` retry
rounds under backoff — tens of minutes is normal, and the ADR 0029 addendum's own
dogfood run measured a single `apt install podman` at 2m43s.

The forces:

- **The client is silent for the whole reconcile.** `fleet apply` prints
  "Applying to <host>…" and then nothing until the reconcile ends (`cli.py:417`).
  golemd logs progress (`fleet logs` tails the WAL brackets, ADR 0029 §2), but the
  applying client shows a frozen line. For a multi-minute apply that is the wrong
  experience — golemd *knows* exactly which glyph it is on, and the client cannot
  see it.

- **A held-open request is fragile.** The reconcile's lifetime is bound to one TCP
  connection. A dropped connection, a client `^C`, or an intermediary idle-timeout
  loses the report — the reconcile keeps running on golemd (it is under the write
  lock, ADR 0020 §3), but the client that started it can never learn the outcome.
  It was forced to guess a read timeout; the just-landed stopgap sets
  `read=None` — an **unbounded** read timeout (`golemd_client.py`, `_APPLY_TIMEOUT`)
  — precisely because no finite value is correct. That removes the timeout symptom
  but keeps the connection coupling: disconnect still loses the result.

- **This blocks parallel apply.** `fleet apply` fans a manifest to hosts
  **sequentially** (`cli.py:416`, the per-record loop), holding each host's request
  to completion before starting the next. `docs/TODO.md` records across-host
  parallelism as the agreed next step and async apply as its prerequisite: you
  cannot cleanly fan out and await N held-open multi-minute requests; you *can*
  fire N cheap 202s and poll N ids.

- **The progress source already exists.** golemd already writes a durable,
  append-only record of exactly where a reconcile is: the WAL (ADR 0020). Every op
  is bracketed `Intended`→`Done`/`Failed`, tagged with its `unit_path` (ADR 0031
  §6) and `step_ord`. "What has golemd done so far, and what is it doing now" is a
  pure fold over the attempt's `wal_step` rows — the same fold recovery and the
  applied-set view already use. No new progress channel is needed; the log is the
  channel.

- **Reads do not need the reconcile's write lock.** The reconcile holds
  `Foreman::write` (`foreman.rs:142`, `:229`) for its whole duration, but the
  `PlanRoom` read methods (`wal_steps`, `latest_attempt`, `revisions`) take only
  `SqlitePlanRoom`'s own short-lived `conn` mutex per query (`planroom.rs:154`,
  `:358`) — never the foreman `write` lock. The reconcile releases `conn` between
  every `append_wal_step`, so a read interleaves with the writes it observes. A
  poll endpoint is therefore lock-free against the reconcile: it never waits on the
  write lock, only briefly on the connection mutex, and sees each bracket as soon
  as it commits.

Framed as unidirectional data flow (`lw:unidirectional-data-flow`): the manifest
flows in, the WAL is the durable log of the fold's progress, and recorded state
plus the report are the outputs. Today the *output* is coupled to the *request
that supplied the input*. This ADR decouples them: ingest returns immediately, and
the WAL — already the source of truth — is projected on demand for both live
progress and the final report.

## Decision

**`POST /manifest` becomes asynchronous, and a new `GET /reconciles/<id>`
projects the WAL into live per-glyph progress plus the final `ReconcileReport`.**
The client fires-and-polls; golemd runs the reconcile on its own and answers "where
are you" from the log.

### 1. `POST /manifest` returns `202 Accepted { reconcile_id }`

The handler does the cheap, synchronous work — decode the manifest bytes, select
this host's scroll, and run the **ingest gate** (recover any interrupted attempt,
then refuse if the latest attempt is unsettled, ADR 0020 §3) — and then **spawns**
the reconcile onto a background blocking task and returns at once:

```
POST /manifest   (raw manifest bytes)
  → 202 Accepted   { "reconcile_id": 42 }
```

The `reconcile_id` is the attempt id `open_attempt` already mints
(`foreman.rs:256`, the `reconcile_attempt` PK, ADR 0020 §2) — returned to the
client before enact begins, so the poll target exists the instant the 202 lands.

**Decode and gate failures keep their current typed non-2xx** `{ kind, message }`
body (ADR 0029 §5, `http.rs:118`): a `ManifestUndecodable` is a typed `500`-class
error (as shipped — the decode failure is reported as a daemon fault, not a
client `400`), an unreadable WAL or an unsettled-attempt conflict is returned
synchronously — because these are reasons the reconcile *never started*, and the
client must learn them on the request that tried to start it, not by polling an id
that will never make progress. The unsettled-attempt gate becomes a **409-style
conflict** (see Consequences) rather than the current internal-error string,
because "another apply is already running on this host" is a well-defined,
client-actionable conflict, not a daemon fault.

Only a reconcile that **actually started** yields a 202 and an id to poll.

### 2. `GET /reconciles/<id>?after=<seq>` — a WAL projection plus an event log

A read-only projection of the attempt's `wal_step` rows (plus the in-memory round
state the WAL cannot carry), lock-free against the running reconcile (§ Context).
It carries **two** things: the folded per-glyph state (the *where are you now*) and
a **sequenced event log** (the *what just happened*, line by line), so a client can
both render a settled tree and stream log lines exactly as golemd produces them:

```
GET /reconciles/42?after=17
  → 200 {
      "reconcile_id": 42,
      "phase": "enacting",              // planning | enacting | settling | settled
                                        //   | rolled_back  (from reconcile_attempt.phase)
      "units": [
        { "unit_path": ["scaly", "fishnet-a"],
          "glyphs": [
            { "glyph_key": "apt:podman",
              "action":    "install",
              "state":     "applied",   // pending | in_progress | applied
                                        //   | unchanged | failed | rolled_back
              "rounds":    1,
              "next_retry_in_ms": null }
          ] }
      ],
      "events": [                       // ordered, > the ?after cursor, empty if none
        { "seq": 18, "at": "…", "level": "info",
          "unit_path": ["scaly", "fishnet-a"], "glyph_key": "apt:podman",
          "message": "install apt:podman" },
        { "seq": 19, "at": "…", "level": "warn",
          "unit_path": ["scaly", "fishnet-a"], "glyph_key": "apt:podman",
          "message": "enact failed (round 1): dpkg lock held; retrying in 2s" }
      ],
      "cursor": 19,                     // the client's next ?after
      "report": null                    // the ReconcileReport, once settled
    }
```

- **`phase`** projects from `reconcile_attempt.phase` (ADR 0020 §2): `planning`
  and `enacting` while in flight, `settling` during config propagation + commit,
  `settled` for a committed attempt, `rolled_back` for a whole-attempt recovery
  rollback.
- **Per-glyph `state`** folds the glyph's `wal_step` rows the same way the
  applied-set view does, with two projection-only states the terminal report does
  not carry: an op with **no row yet** is `pending`; an `Intended` row with no
  terminal successor is `in_progress`; and the terminal states reuse ADR 0029's
  vocabulary — `applied`, `unchanged` (a `Noop`), `failed`, `rolled_back`. `rounds`
  is the retry-round count from the op's repeated `Intended`→`Failed` brackets.
- **`next_retry_in_ms`** is the one field the WAL **cannot** carry: the countdown
  to the next retry round lives only in the in-memory round loop
  (`enact_unit`/`round_delay`, `foreman.rs:395`, `:123`) — the WAL records that an
  op `Failed`, never that a retry is scheduled in N ms (ADR 0029 §1 keeps the retry
  *class* and pace in memory by design). The projection reads it from the live
  round state when present, and omits it otherwise. This is a deliberate, small
  seam: the durable rows carry the settled truth; the in-memory state supplies only
  the transient countdown.
- **`report`** is `null` until the attempt settles, then the full `ReconcileReport`
  (ADR 0029 §5, unchanged shape). Once `phase = settled`/`rolled_back`, a poll
  returns the identical report the synchronous 200 used to return — same
  `revision`, `outcome`, `units`, `glyphs`, `failures`.

#### The event log — where the lines come from

`events` is the same facts golemd **already logs** to its journal (ADR 0029 §2 —
manifest ingested, per-key install/replace/remove, enact-failed round N with its
reason, giving-up, the rollback steps, forensics captured, revision recorded),
now **also** handed to the client as ordered records so it renders each line the
instant golemd emits it. Two sources feed it, and the split is deliberate:

- **From the WAL where the fact is durable.** The op brackets (`Intended`→
  `Done`/`Failed`, tagged with `unit_path`/`glyph_key`, ADR 0031 §6) already carry
  "started install of apt:podman", "apt:podman done", "apt:podman failed". Those
  events are *derived from the WAL rows* — they survive a daemon restart because
  the rows do, and they carry the same `seq`-able ordering the fold already uses.

- **From an in-memory per-attempt event buffer for what the WAL does not carry.**
  The retry-round **delay** and the failure **reason** live only in the live round
  loop — the WAL records that an op `Failed`, never *why* or *that a retry is
  scheduled in N ms* (ADR 0029 §1 keeps the retry class and pace in memory; the
  0029 revision put the failure **reason** in memory per attempt too). golemd holds
  a **bounded, cursor-keyed ring** of these events for the running attempt and
  serves the slice `> ?after`. It is small, ordered, and **lost on daemon restart**
  — and that loss is acceptable: on restart, recovery (ADR 0020 §3) re-drives from
  the WAL and the **states** are reconstructed in full; only the transient
  round-delay/reason *lines* for the pre-crash rounds are gone, and a reattaching
  client resumes streaming from the recovered attempt's WAL-derived events. Record
  this honestly: **states are durable, the finest log lines are best-effort.**

The `?after=<seq>`/`cursor` pair makes the log a resumable stream over a plain GET:
a client passes back the `cursor` it last saw and receives only newer events, so a
poll that drops and reconnects re-requests from its cursor and misses nothing the
buffer still holds (and nothing at all, for the WAL-derived events).

#### The `kind` split — lifecycle vs. command output

Events carry an additive **`kind`** field. `"lifecycle"` is everything §2 already
describes — the manifest-ingested, install/replace/remove, enact-failed-round-N,
giving-up, rollback, forensics, and revision-recorded lines that mark *what golemd
decided*. `"cmd"` is the new tier: the **raw stdout/stderr lines of the host
commands golemd runs** — `apt-get update`, `apt-get install`, `systemctl
daemon-reload`/`enable`/`start`, and the reverse and diagnose commands — forwarded
line by line *while the command runs*, so the TUI shows the real build/install
output, not just the glyph verdict. A record with no `kind` reads as `"lifecycle"`
(the field is additive; old readers and pre-`kind` rows keep their meaning):

```
{ "seq": 24, "kind": "cmd", "level": "info",
  "unit_path": ["scaly","fishnet-a"], "glyph_key": "apt:podman",
  "message": "Get:1 http://deb.debian.org/debian bookworm/main podman amd64 4.3.1" }
```

Both kinds land in the **same per-attempt ring** (`progress.rs`, §2), share the
one `seq` cursor, and are the **same best-effort tier** — held in memory, **never
in the WAL**, lost on daemon restart. A reattaching client's states are
reconstructed from the WAL in full (ADR 0020 §3); the pre-crash `cmd` lines simply
do not survive, exactly as the round-delay/reason lines do not. Command output is
progress texture, not a durable record — the WAL brackets remain the settled truth.

**The port shape — a default streaming method, opt-in.** The `CommandRunner` port
(`host.rs`) gains a **streaming variant with a default implementation** that falls
back to the existing `run()` and forwards nothing mid-command:

```
fn run_streaming(&self, program, args, sink: &mut dyn FnMut(EventLevel, &str))
    -> EnactResult<CommandOutput>
{ let out = self.run(program, args)?; Ok(out) }   // default: no streaming
```

- The **`SystemCommandRunner`** overrides it: it spawns with piped stdout/stderr,
  reads each stream line by line, calls `sink(level, line)` as lines arrive
  (stdout→`info`, stderr→`warn`), and still returns the captured `CommandOutput`
  for the reconciler's existing success/stderr checks. The reconciler's `sink`
  closure `record`s each line into the ring tagged `{unit_path, glyph_key,
  kind:"cmd"}`.
- The **`FakeCommandRunner`** and every existing test **inherit the default** and
  are untouched — they emit no `cmd` events unless a test opts in by overriding
  `run_streaming`. The port change is additive: no existing `run()` call site
  changes, and the streaming path is taken only by the reconciler adapters that
  choose to pass a sink (apt/systemd apply, reverse, diagnose).

**The honest boundary — only golem-invoked commands stream.** golemd streams only
the output of the commands **it** spawns. Output produced **inside** systemd — a
Podman quadlet's image pull under `systemctl start`, a unit's own logs — stays
invisible to the `cmd` tier, because golemd sees only that command's stdout/stderr,
not the child processes systemd owns (the same §4 granularity boundary: golemd's
unit of observation is the reconciler call). A `systemctl start` that blocks on a
multi-minute pull shows its own sparse output plus the glyph's elapsed time, not
the pull's progress. **Forensics still cover the failure**: `diagnose` (§ ADR 0029)
already captures `systemctl status`/`journalctl` on a failed unit, so the pull's
error surfaces in the report even though its live output did not stream.

**ADR 0020 logging discipline holds for `cmd` events.** Command output may echo
package names, versions, and unit names — those are already in the glyph keys and
are fine to stream (ADR 0029 §2: keys and reasons are loggable). What must **never**
stream is **file contents**: the `file`/`lineInFile` reconcilers do real filesystem
I/O and run **no output-producing command** — they never call the runner, so they
produce **no `cmd` events at all**, and the "never log file contents or secrets"
rule (ADR 0029 §2, the ADR 0020 discipline) is preserved structurally rather than
by filtering. Only the apt/systemd command adapters stream, and package/unit output
is not secret material.

**`GET /reconciles/latest`** returns the same shape for the most recent attempt —
the reattach convenience for a client that lost its id (a crash, a new terminal).

**Concurrency stance (recorded):** the poll path takes **no** foreman `write` lock.
It calls `PlanRoom` reads (`wal_steps_for`, `latest_attempt`), each of which locks
only the sqlite `conn` mutex for the duration of one query and releases it
(`planroom.rs`). The reconcile holds `Foreman::write` for its whole run but
likewise releases `conn` between every `append_wal_step`. So a poll **never blocks
on the reconcile's write lock** and never delays it; the two interleave on the
connection mutex alone, and a poll observes each WAL bracket as soon as its
transaction commits. This is the ADR 0020 property that the applied-set view is a
pure fold over an append-only log, read on demand — reused verbatim for progress.

### 3. `golemctl apply` is the primary live client — a devenv-style progress TUI

The **live renderer is golemctl** (`apps/golemctl/src/main.rs`), not fleet. golemctl
fires the 202, takes the `reconcile_id`, and polls `GET /reconciles/<id>?after=<cursor>`
on a ~1s interval, rendering the projection as a **live progress tree** built on the
**same progress system devenv uses** — its `iocraft`-based declarative TUI
(`devenv-tui/`). golemd is the reconciler's unit of observation; golemctl is the
product surface a person watches while it runs.

**What devenv-tui is (the pattern being adopted).** An Elm-shaped, iocraft-rendered
TUI with a clean split between a **model**, the **events** that mutate it, and a
**view** that is a pure function of the model:

- **Model** (`devenv-tui/src/model.rs`): an `ActivityModel` — a tree of `Activity`
  nodes keyed by id, each with a `parent_id`, a `NixActivityState`
  (`Queued`/`Active`/`Completed { success, .. }`), a variant (task/build/…), and a
  per-node ring of log lines. A sibling `UiState` (selection, scroll, view mode)
  lives *outside* the model's lock — model = activity facts, UiState = view
  interaction.
- **Events** (`devenv-tui/src/model_events.rs`, and the activity ingest in
  `model.rs`): typed events arrive on a channel and `apply` to the model — spans
  become nodes (`create_activity`), span-close becomes `handle_activity_complete`
  (flipping a node to `Completed { success }`), log events append to the node's
  buffer (`handle_activity_log`). golem's poll response replaces devenv's
  `ActivityEvent` mpsc as the event source.
- **View** (`devenv-tui/src/view.rs`, `components.rs`): a pure render of the model
  tree — each node a row with a **status glyph** and, beneath the **active** node,
  its recent log lines (`ChildActivityLimit` bounds how many, with a linger). The
  status glyph is a **self-animating `Spinner`** (`components.rs`, `SPINNER_FRAMES`)
  while `Active`, and a `StatusIndicator` that resolves to `CHECKMARK` (`✓`) on
  success or `XMARK` (`✗`) on failure once `Completed`. Rendering is throttled
  (`throttled_notify_loop`, `lib.rs`) to a max FPS; it renders to **stderr** and
  **does not take the alternate screen**, so scrollback is preserved — exactly the
  behaviour an apply wants.

**Where golem diverges from devenv-tui.** devenv-tui keeps each node's log lines
**in memory only** — an `Arc<VecDeque<String>>` per activity, capped at
`max_log_lines_per_build` (default 1000, `app.rs`), split into
`log_stdout_lines`/`log_stderr_lines` — and renders them under the active node. It
**never writes those lines to a file and never advertises a log path**: when the
process exits, the buffers are gone, and a person who wants to grep the run after
the fact has nothing on disk. golem adopts the model/events/view shape but adds
on-disk persistence and a header path (decisions 1–2 below), because an apply is a
change to a real host that an operator will want to re-read and grep long after the
spinner cleared.

**golem's mapping.** The recursive scroll makes "a spinner per scroll" and "a spinner
per unit" the *same statement*: each `unit_path` in the projection is one tree node.
So the mapping is direct:

- **One node per (sub-)scroll/unit**, nested by `unit_path` — the projection's
  `units` array *is* the activity tree.
- **A spinner on each unit while any of its glyphs is `pending`/`in_progress`**;
  under the active unit, the **`events` lines stream in** as that node's log buffer,
  exactly as they come through the poll.
- **Per-glyph state → the settled glyph**: `✓` applied · `·` unchanged · `↩` rolled
  back · `✗` failed (with its failure **class** from the report/`events` reason), and
  the animated spinner for `in_progress`, plus a dim `next_retry_in_ms` countdown
  while a retry is scheduled.
- When `phase` reaches `settled`/`rolled_back`, golemctl stops polling and **prints
  the final `ReconcileReport` exactly as today** (its existing pretty-print) — the
  tree was the live view; the report is the settled record.

#### 3d. Buildkit-style glyph tails — the last 3 command lines under the active glyph

Under each **active** (`in_progress`) glyph row, the TUI renders the **last 3
`kind:"cmd"` lines** for that glyph — dim, indented one level under the glyph,
rolling as new lines arrive — the way buildkit shows the tail of a running step's
output beneath the step. So `apt:podman` mid-install reads:

```
  ⠹ apt:podman  install
      Unpacking podman (4.3.1) ...
      Setting up conmon (2.1.6) ...
      Processing triggers for man-db ...
```

- **The tail is per-glyph and scoped to `kind:"cmd"`.** `kind:"lifecycle"` lines
  keep their existing homes — the per-unit/host event regions of §3 (the node's
  streamed log buffer and the failure/retry text on the glyph row). The `cmd` tail
  is a *separate*, glyph-local rolling window, not the same buffer.
- **It collapses on glyph completion.** When the glyph settles (`applied`/
  `unchanged`/`failed`/`rolled_back`) the three-line tail disappears and the row
  resolves to its status glyph, keeping the settled tree compact — the command
  transcript is not lost, it lives on in the §3a tmp files. A **failed** glyph is
  the one case worth surfacing the tail's residue: the failure **reason** (a
  lifecycle event) already renders on the row per §3; the raw `cmd` lines that led
  to it are in `<unit>.log`.
- **Three is the on-screen window, not the retention.** The ring (§2) and the tmp
  files (§3a) hold the full command output; the TUI shows only the freshest three
  per active glyph so N concurrent glyphs stay readable on one screen.

#### 3a. Event-log persistence — every event line is written to a local tmp file

golemctl writes **every `events` line it receives** — **both `kind`s** — to local
tmp files as it polls, so the whole run is greppable on disk after the spinner
clears. With `cmd` events landing here too, `<unit>.log` becomes the **full command
transcript** of that unit's apt/systemd work, not just its lifecycle verdicts — the
buildkit tail (§3d) shows the freshest three on screen; the file keeps them all,
strengthening the §3a grep story. Two tiers of file, written together:

- **One combined `all.log`** — every event, in poll order.
- **One filtered file per unit** — the events tagged with a given `unit_path`,
  including the `<removes>` group, so a run can be read one scroll/unit at a time.

The layout under a per-apply directory:

```
$TMPDIR/golemctl/apply-<reconcile_id>/
  all.log
  <unit-file>.log          # one per distinct unit_path
```

`$TMPDIR` falls back to `/tmp` when unset. `<reconcile_id>` is the attempt id from
the 202 (§1), so concurrent applies never collide.

**Unit-path filename encoding.** A `unit_path` is a list of segments (ADR 0031 §6).
It is encoded into **one flat filename** by joining segments with `-` and suffixing
`.log`: `["scaly","fishnet-a"]` → `scaly-fishnet-a.log`. A flat file-per-unit (not a
mirrored directory tree) keeps `ls` and `grep *.log` trivial and sidesteps having to
recreate the scroll's nesting on disk. Segments are slugged for the filesystem — any
character outside `[A-Za-z0-9._-]` is replaced with `_` — which resolves the
`<removes>` group: `["<removes>"]` writes to **`_removes_.log`**, because `<` and `>`
are filesystem-legal but shell-annoying (a bare `removes.log` glob or a `<` redirect
in a `grep` line is a foot-gun). The encoding is lossy-but-stable — two paths that
slug to the same name would share a file; unit paths in practice are already
slug-safe, so this is recorded as a known, tolerated edge, not guarded against.

Each line is **plain text, one event per line**, carrying `timestamp`, `level`,
**`kind`**, `glyph_key`, and the message — the same fields the projection's `events`
records carry (§2), now with the `kind` column so lifecycle and command lines are
distinguishable when the two interleave. `all.log` **interleaves both kinds in poll
order** and carries the `kind` column so a `grep cmd` / `grep lifecycle` (or an
`awk` on the column) separates them after the fact — **decided: one more column, not
two files.** A single ordered transcript with a filter column beats splitting
`all.log` into `all.lifecycle.log`/`all.cmd.log`, which would lose the interleaved
ordering that shows *which command output preceded which verdict*. Formatted for
`grep`, e.g.:

```
2026-07-26T14:03:10Z  info  cmd        apt:podman  Unpacking podman (4.3.1) ...
2026-07-26T14:03:11Z  warn  lifecycle  apt:podman  enact failed (round 1): dpkg lock held; retrying in 2s
```

The files **survive after the apply exits** — they are the after-the-fact record,
the thing devenv-tui does not keep. There is **no rotation and no cleanup**: tmp is
the lifecycle (the OS reclaims `$TMPDIR` on its own schedule), and an apply's log is
small. Persistence runs on **both** paths — the TUI and the non-TTY/`--json` fallback
write the identical files; the files are not a TTY feature.

#### 3b. The log directory is advertised above the spinners

The TUI renders a **header line above the tree**, present **from the first frame**
(before any unit node exists), naming the directory the event logs are being written
to:

```
logs: /tmp/golemctl/apply-42/
```

It sits above every spinner so a person can `cd`/`grep` there while the apply is still
running, not only after it settles. The **plain / non-TTY path prints the same line
once at start** — the first thing golemctl emits before the event lines — so a piped
or CI run records where its logs went too.

#### 3c. Nested spinners are first-class — the model is a real tree

The renderer model is a **real tree that mirrors the recursive scroll**, not a flat
list of unit rows. The flat unit-row rendering sketched above is **superseded** by
this: each path prefix in the set of `unit_path`s is its own **branch node** with its
own spinner/status row, and its children indent beneath it — the same shape
devenv-tui's activity tree already renders, now keyed by `unit_path` prefix rather
than a flat `units` array.

- **Branch nodes are first-class.** A prefix like `["scaly"]` that no glyph attaches
  to directly still renders as a node with a spinner and an aggregated status; its
  child units nest under it. Leaf nodes (a `unit_path` that carries glyphs) render
  their glyph rows as before.
- **A branch's state AGGREGATES its subtree**, reusing the per-glyph/unit vocabulary
  of §2 (`pending`/`in_progress`/`applied`/`unchanged`/`failed`/`rolled_back`) so the
  tree speaks one status language top to bottom. The aggregation rule, in precedence
  order:
  - **active** (spinner) if **any** descendant is `pending`/`in_progress`;
  - else **failed** if **any** descendant is `failed` terminally;
  - else **rolled_back** if the subtree rolled back (any descendant `rolled_back`,
    none still failing);
  - else **settled** — resolved to `applied` when the subtree did work, `unchanged`
    when every descendant was a `Noop` — once **all** descendants are settled.
  A branch spins while anything under it is still moving and resolves to the
  worst terminal state beneath it once everything has stopped.
- **The `<removes>` group and single-leaf roots render naturally** as tree nodes: the
  `<removes>` group is one branch (its removals nesting under it, logged to
  `_removes_.log` per 3a), and a scroll with a single leaf unit is a one-node tree —
  no special case, the tree degenerates cleanly.

A **disconnected client re-polls the same id and loses nothing** durable — the WAL
holds the whole state history; `golemctl apply --reattach` (or a `fleet status`) hits
`GET /reconciles/latest` and rebuilds the tree from the projection, resuming the event
stream from a fresh cursor.

**Non-TTY / `--json` falls back to plain line output** — no iocraft, no spinner: each
`events` line printed as it arrives, then the report (or its JSON) at the end. The TUI
is for a human at a terminal; a pipe or CI gets deterministic lines.

**Dependency decision (recorded).** golemctl takes a **new `iocraft` dependency** —
the crate devenv-tui is built on. devenv-tui is a **devenv-internal crate, not
published to crates.io**, so golem **adopts the pattern, not the crate** (see
Alternatives 5). Pin **the same `iocraft` major the devenv workspace vendors**:
devenv 2.1.1's workspace pins `iocraft = "=0.8.2"` (with a git patch to `main` for an
unstable stderr-rendering feature, `unstable-output-streams`); golem takes `iocraft`
at that `0.8` line. Whether golem needs the same git-patched build or can ride the
released `0.8.2` is an implementation detail (golem does not need devenv's stderr-PR
patch unless it hits the same gap) — left to the build-out; the **major/line is the
commitment recorded here**.

### 4. Granularity honesty (recorded limitation)

Progress is **per-glyph, and no finer**. golemd sees a glyph transition
`Intended`→`Done`/`Failed`; it does **not** see inside `systemctl start`, where a
Podman image pull happens under systemd's control, nor inside a long `apt install`.
So a glyph that takes minutes shows as `in_progress` with **elapsed time**, not a
byte-level or percent progress bar. This is a **known limitation, not a gap to
fix**: golemd's unit of observation is the reconciler call, and the four-glyph
model deliberately delegates the work inside a `systemdService`/`aptPackage` to
systemd/apt, which golem does not instrument. A finer bar would mean golemd parsing
systemd/apt/podman progress output — new coupling to those tools' formats for a
cosmetic gain. Rejected; elapsed-time-per-glyph is the honest granularity.

### 5. fleet delegates apply rendering to golemctl — one TUI, two surfaces

`fleet apply` **execs the locally-built golemctl** against each host's forwarded
port instead of re-implementing the live TUI in Python. There is **one** live
renderer — golemctl's iocraft tree (§3) — reached through **two** surfaces:
`golemctl apply` directly, and `fleet apply` which drives golemctl per host. fleet
keeps exactly what it owns today and nothing more:

- **fleet still compiles, selects hosts, and ships the manifest** — `fleet apply`
  compiles the `.emet` source to a manifest (`deploy_ops.compile_manifest`,
  `cli.py:411`), resolves the target records (`_target_records`), and for each host
  **execs `golemctl apply` pointed at that host's `127.0.0.1:<golemd_port>`**,
  handing it the compiled manifest. golemctl runs the fire-then-poll protocol and
  **renders the tree and prints the report itself**. The per-host apply body in
  `cli.py` (the `apply_manifest` call, the status-code branch, and the
  `_render_report` call, `cli.py:416`–`438`) is **replaced by a golemctl exec**.
- **The Python `_render_report` is retired from the apply path**, kept only as the
  fallback report printer for an HTTP path where golemctl is unavailable (a host
  reachable over the forwarded port but no golemctl binary to hand it). golemctl
  prints the report on the normal path; `_render_report` stops being the apply
  renderer and becomes a fallback-only function (or is removed if the fallback is
  dropped — see Open questions). The point recorded: **the TUI is not duplicated in
  Python.**
- **Non-TTY fleet runs pass `--json` through to golemctl**, which emits plain lines
  and JSON (§3) — fleet does not re-render those either.

### 6. Prerequisite for parallel apply

Async apply is what makes **across-host parallel apply** a clean fleet-side loop,
and this ADR **absorbs the `docs/TODO.md` async-apply line and the across-host
half of the parallel-apply line**:

- **Cross-host parallelism is a fleet-side change, no further golemd work.** With
  ingest returning a cheap 202, `fleet apply` fans out to hosts concurrently —
  each host is one `golemctl apply` exec against its forwarded port, so N hosts is
  N concurrent golemctl processes (each running its own fire-then-poll), replacing
  the sequential per-record loop (`cli.py:416`). Rendering N live trees at once is a
  golemctl/terminal concern, not new golemd work; the hosts are independent
  machines and the daemon side is already N cheap POSTs + N polls.
- **Within-host parallel units stay future work.** Running a host's leaf units
  concurrently inside golemd still requires serializing the shared, non-reentrant
  resources two units contend on — the single dpkg lock and the apt index for
  `aptPackage`, and per-file write dedup for `lineInFile` (`docs/TODO.md`, Dr. Dub's
  constraints). That is gated on that serialization work and is **not** in scope
  here; this ADR only decouples ingest from completion, which is the shared
  prerequisite for both kinds of parallelism.

### 7. Compatibility — lockstep, and the stopgap is removed on landing

The fleet harness, golemctl, and golemd **ship in lockstep** (ADR 0013), so there is
**no dual-protocol support and no migration window**: the synchronous 200-with-report
transport is **replaced**, not deprecated-alongside, and every client that spoke it is
updated in the **same change**.

- **golemctl** (`apps/golemctl/src/main.rs`) today posts to `/manifest` and, in
  `print_response`, `bail!`s on non-2xx and prints the body as the whole result — it
  assumes a single synchronous body. It becomes the **fire-then-poll live client of
  §3**: read the 202 `{ reconcile_id }`, poll `GET /reconciles/<id>?after=<cursor>`,
  render the iocraft tree, and print the final `report` on settle.
- **fleet** stops speaking the protocol at all on the apply path — it **execs
  golemctl** (§5). `golemd_client.apply_manifest` and its unbounded `_APPLY_TIMEOUT`
  are removed from the apply path (kept only if the HTTP fallback of §5 is kept).

**The stopgap is explicitly transitional and is removed when this lands.** The
just-landed unbounded read timeout (`read=None` in `golemd_client._APPLY_TIMEOUT`,
commit ffa1414) was a correct "don't time out mid-apply" patch and **nothing more**.
When this ADR lands, **both** the synchronous held-open request **and** the unbounded
wait go away together: the POST is a fast 202, and the waiting is cursor-based polling
of cheap bounded reads. There is no version in which the sync path and the async path
coexist.

## Alternatives considered

1. **Keep synchronous + unbounded client timeout (the just-landed stopgap).**
   Rejected as the end state. `read=None` (`golemd_client.py`) removes the
   *timeout* symptom but not the *coupling*: the reconcile's result still lives on
   one held-open connection, so the client is silent for minutes and any
   disconnect (client `^C`, dropped link, intermediary idle-close) still loses the
   report while the reconcile runs on regardless. It is a correct stopgap for "don't
   time out mid-apply" and is cited here as exactly that — not a design.

2. **SSE / chunked streaming of progress over the apply request.** Rejected: more
   plumbing (a streaming response type, a server-side event pump, client stream
   parsing) for **no resume-after-disconnect** — a dropped SSE stream is a lost
   stream, the same fragility as the held-open request. Polling a WAL projection is
   strictly simpler: the WAL *already exists* and is *already* the durable
   progress record, so the projection is a pure read with no new server-side
   streaming machinery, and a client that reconnects just re-polls the id and
   catches up from the log. Resume-after-disconnect is free; with streaming it is
   extra work.

3. **WebSocket progress channel.** Rejected as overkill: a stateful bidirectional
   socket for what is a one-directional, pull-when-you-want status read. It adds a
   connection lifecycle and a framing protocol to solve a problem a periodic GET
   solves, and still has no better disconnect story than re-polling.

4. **Poll `GET /status`/`GET /revisions` instead of a new endpoint.** Rejected:
   those answer "what is the latest committed revision," not "how far is the
   *in-flight* attempt." The live per-glyph, per-unit state with retry countdowns
   is a distinct projection (the `pending`/`in_progress` states and
   `next_retry_in_ms` do not exist in a settled revision), and it is keyed on a
   specific `reconcile_id` a client holds from its 202. A dedicated
   `GET /reconciles/<id>` keeps the read routes each answering one honest question.

5. **Depend on `devenv-tui` directly rather than adopting its pattern.** Rejected —
   not by preference but by availability. `devenv-tui` is a **devenv-internal
   workspace crate, not published to crates.io** (its `Cargo.toml` carries no
   independent version and pulls sibling devenv crates by `.workspace`), so there is
   no crate to depend on. golem therefore adopts devenv-tui's **architecture** —
   model/events/view, spinner-per-node, log-lines-under-the-active-node — and takes
   the one **published** dependency underneath it, `iocraft`, at the same `0.8` line
   the devenv workspace pins. Pattern adoption plus an `iocraft` dependency, not a
   `devenv-tui` dependency.

7. **Keep log lines in memory only, as devenv-tui does.** Rejected: devenv-tui's
   per-node buffers vanish when the process exits, leaving nothing to grep after an
   apply. golem writes the event lines to tmp files (§3a) and advertises the path
   (§3b) precisely to keep the after-the-fact record devenv drops. The extra cost is
   a line-append per event.

6. **Re-implement the live spinners in Python (rich `Live`) for fleet.** Rejected:
   two TUIs to build and keep in step for the same picture. golemctl is the product
   surface and already must run the fire-then-poll protocol; a Python renderer would
   duplicate the tree, the spinner, the per-glyph vocabulary, and the report layout,
   and drift from golemctl's. fleet instead **execs golemctl** (§5) — one TUI, two
   surfaces. The Python renderer survives only as the report printer on an HTTP
   fallback where golemctl is unavailable (Open questions), never as a second live
   TUI.

## Consequences

- **The client sees live progress and survives disconnects.** `golemctl apply`
  renders a per-glyph live iocraft tree from the projection (§3), streaming the
  `events` log under each active unit; a client that drops its connection re-polls
  the id (or `latest`) from its cursor and loses nothing durable — the reconcile is
  no longer coupled to one TCP connection.
- **Every apply leaves a greppable log on disk.** golemctl writes each event line to
  `$TMPDIR/golemctl/apply-<id>/` — a combined `all.log` plus a per-unit file (§3a) —
  and advertises that directory above the spinners (§3b). Trivial extra IO (a
  line-append per event to two files) buys an after-the-fact record devenv-tui does
  not keep; no rotation, tmp is the lifecycle.
- **The renderer model is a real scroll tree, not a flat unit list.** Branch nodes
  (path prefixes) get their own spinner and an aggregated subtree status (§3c),
  superseding the flat per-unit fold; the `<removes>` group and single-leaf roots
  fall out as ordinary tree nodes.
- **Apply is a two-request, cursor-polled protocol.** `POST /manifest` →
  `202 { reconcile_id }`, then `GET /reconciles/<id>?after=<cursor>` until settled.
  Every client (golemctl directly, fleet via golemctl, any future one) follows the
  fire-then-poll shape; the single-request "POST and read the report" is gone. This
  is the main thing the change forecloses.
- **One live TUI, reached two ways.** golemctl owns the iocraft renderer; `fleet
  apply` execs golemctl per host rather than re-rendering in Python (§5). fleet keeps
  compile + host-selection + manifest-shipping; the Python `_render_report` retires
  from the apply path (fallback-only, or removed). golemctl gains an `iocraft`
  dependency at devenv's `0.8` line.
- **A second concurrent apply on one host is a typed conflict.** The unsettled-attempt
  ingest gate (`foreman.rs:247`), today an `Internal` error string, becomes a
  **`409 Conflict`** with a typed body `{ kind: "reconcile-in-progress", message,
  reconcile_id }` — the id of the attempt already running, so the caller can poll
  *it* instead of retrying. This makes "one apply at a time per host" (the ADR 0020
  write-lock invariant) a first-class, pollable client contract rather than an
  opaque 500.
- **Progress granularity is per-glyph, capped by design.** A long `systemctl start`
  or `apt install` shows elapsed time on an `in_progress` glyph, never a byte/percent
  bar — golemd does not instrument inside systemd/apt/podman (§4). Recorded as a
  known limitation.
- **Across-host parallel apply becomes a fleet-side fan-out** (one golemctl exec per
  host) with no further golemd change (§6); within-host parallel units remain gated on
  the apt/dpkg and `lineInFile` serialization work and stay out of scope.
- **States are durable, the finest log lines are best-effort.** The `events` log is
  WAL-derived where the fact is durable (the op brackets survive restart) and drawn
  from a **bounded, cursor-keyed in-memory ring** for what the WAL never carries —
  round delays and failure reasons (§2). That ring is **lost on daemon restart**: the
  per-glyph **states** are reconstructed in full by recovery (ADR 0020 §3), but the
  transient round-delay/reason *lines* for pre-crash rounds do not survive. Recorded
  as an accepted asymmetry, not a defect — a reattaching client always gets correct
  states and resumes the stream from the recovered attempt's WAL-derived events. The
  new `cmd` lines (§2 `kind` split, §3d) join this best-effort tier — durable states,
  ephemeral command output.
- **Command output streams live, but volume rises sharply.** A single `apt install`
  can emit **hundreds** of `cmd` lines (unpacking, setup, triggers). Both kinds share
  one ring (§2), so a chatty install can push the ring past its bound and **evict
  earlier `lifecycle` events before a slow client reads them** — losing the very
  install/failed/giving-up lines the tree needs. **Mitigation, decided: per-kind ring
  bounds, not one shared cap.** The ring keeps a **separate bound per `kind`** — the
  small lifecycle stream (a handful of events per glyph) gets a modest cap it will
  never exhaust, and the high-volume `cmd` stream gets its own larger cap and evicts
  only *itself* when it overflows. Lifecycle events can no longer be crowded out by
  command chatter; a `cmd` flood drops old `cmd` lines (still on disk in §3a) and
  leaves lifecycle intact. This supersedes the single-`EVENT_RING_CAP`/shared-cursor
  shape of the shipped §2 ring (`progress.rs`), which must gain the per-kind split;
  the shared monotone `seq` cursor is kept (one ordered stream to the client), only
  the **eviction bound** becomes per-kind.
- **The `CommandRunner` port grows a streaming method, default off.** `run_streaming`
  lands with a default that delegates to `run()` and emits nothing (§2), so the
  `FakeCommandRunner` and every existing reconciler test compile and pass unchanged —
  the change is additive. Only `SystemCommandRunner` overrides it (piped spawn,
  line-forwarding sink) and only the apt/systemd apply/reverse/diagnose adapters pass
  a sink; the `file`/`lineInFile` reconcilers never call the runner, so they stream
  nothing and cannot leak file contents (ADR 0029 §2 / ADR 0020 discipline, upheld
  structurally). A test that wants to assert on `cmd` events opts in by overriding the
  method on its fake.
- **One in-memory seam in the projection.** `next_retry_in_ms` is read from the
  live round loop, not the WAL (the WAL never records a scheduled retry, ADR 0029
  §1). If golemd restarts mid-reconcile, that field is simply absent on the recovered
  attempt's projection — recovery (ADR 0020 §3) re-drives from the WAL brackets, and
  the poll then reports the **recovered** outcome (the re-driven `Done`/`Failed` and
  the eventual settle), which is exactly what a reattaching client wants. Crash
  recovery is already covered by ADR 0020; this ADR adds only that the poll endpoint
  surfaces its result.
- **The report shape and in-band-failure philosophy are untouched** — ADR 0029 §5
  stands in full; a partial or rolled-back reconcile is still a *result* carried in
  `outcome`/`failures`, now delivered on the final poll's `report` instead of the
  apply response. Only genuine "the reconcile never started" errors (undecodable
  manifest, unreadable WAL, in-progress conflict) are non-2xx, on the `POST` itself.
- **golemctl grows a live TUI and an `iocraft` dependency.** golemctl is no longer a
  thin POST-and-print client; it carries the model/events/view renderer (§3) and its
  `iocraft` dependency at devenv's `0.8` line. This is new weight in golemctl's build
  and the price of a single, shared live surface — accepted deliberately over a second
  Python TUI.
- **The stopgap is removed, not layered over.** The synchronous held-open transport
  and the unbounded `read=None` timeout (ffa1414) both go away when this lands (§7);
  there is no coexistence window.
- **Cross-references:** supersedes in part ADR 0029 (transport only — the report
  shape, tree structure, and in-band failures are preserved), builds on ADR 0020
  (the WAL is the projected progress source; the write-lock invariant becomes the
  409 conflict; crash recovery reports through the poll) and ADR 0031 §6 (the WAL's
  `unit_path` shapes the per-unit projection), and ships in lockstep per ADR 0013
  (fleet, golemctl, and golemd updated together, no dual protocol). The four-glyph
  contract and the manifest wire format are unchanged — `GET /reconciles/<id>` is
  golemd's HTTP surface, not the `scroll-format` contract. The live TUI adopts the
  `devenv-tui` architecture (model/events/view, spinner-per-node) as a pattern, not a
  code dependency (Alternative 5).

## Open questions

- **Terminal-attempt retention for `latest`/`<id>` polling.** A reattaching client
  polls after settle, so a settled attempt's rows must survive long enough to be
  read. ADR 0020's compaction/retention (alternative 3 there) prunes superseded
  steps behind a checkpoint; the retention window must keep the *latest* settled
  attempt's rows readable for at least a reattach interval. Left to the ADR 0020
  retention policy to fix concretely; flagged here as a dependency.
- **Poll interval and terminal backoff.** ~1s is the proposed cadence; whether the
  client backs off once `phase` is terminal (one final read then stop) versus a
  fixed interval throughout is a small client-side detail left to implementation.
- **Event-buffer size bound.** The in-memory per-attempt event ring (§2) needs a
  concrete cap — lines held per attempt before the oldest are dropped. Too small and
  a slow client polling at ~1s misses lines that scrolled past the cursor between
  polls; too large and a chatty rollback bloats daemon memory. A per-attempt bound in
  the low thousands (matching devenv-tui's `max_build_logs` default of 1000 per node)
  is the starting guess; the exact number and whether it is per-unit or per-attempt is
  left to implementation.
- **How fleet resolves the golemctl binary.** `fleet apply` execs golemctl (§5), so it
  must find one. Whether it takes the devenv-provided script on `PATH`, the cargo
  `target/…/golemctl` from a workspace build, or an explicit configured path — and
  what it does when none is present (the HTTP-fallback report path, or a hard error) —
  is unsettled. This also decides whether the Python `_render_report` and
  `golemd_client.apply_manifest` stay as an HTTP fallback or are removed outright.
