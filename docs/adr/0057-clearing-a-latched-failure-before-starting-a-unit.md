# 0057 — golem clears a latched failure before it starts a unit

## Status

Accepted 2026-08-11 (decision by Dr. Dub, after a production incident). Amends
the alternative
[ADR 0036](0036-plan-verb-and-unit-notifies-reload.md) rejected under
"`reload-or-restart` (start-if-inactive) instead of `try-`": the `try-` verbs
still govern a merely inactive unit, and only a *failed* one gets the forcing
verb. The amendment is real and not merely a narrowing — the gate is the unit's
state rather than the scroll's declaration, so it reaches units no
`systemdService` glyph declares, which the Decision states outright. Refines
[ADR 0020](0020-write-ahead-reconcile-log.md) §5's structural restart bracket
(`daemon-reload` + `try-restart`) and the systemd apply addendum of
[ADR 0015](0015-reversible-reconcilers-and-content-addressed-versioning.md) by
the same rule; both carry a pointer back. Implemented in
`apps/golemd/src/reconcilers.rs`.

Answered in part by
[ADR 0058](0058-the-plan-reads-the-host-and-only-a-verdict-crosses-the-port.md)
(2026-08-11): the open question in the Consequences below — whether golem should
look for latched units outside a diff-driven op — is settled in the affirmative
**for reporting only**. `golemctl plan --against-host` observes every desired
glyph, `Noop`s included, so a unit that is inactive or latched failed is
reported `Divergent` even where the diff has nothing to enact. Nothing there
enacts anything, and the verbs, paths, and blast radius this ADR governs are
unchanged.

## Context

Three sites went down and stayed down while golem reported the reconcile green.

The units had hit `StartLimitBurst` while the network was missing, which latches
`Active: failed`. A unit latched that way refuses every subsequent start job
until `systemctl reset-failed` clears it — the refusal reads *"Job for the unit
failed because start request repeated too quickly."* Nothing in golem's path
cleared it.

The reconcile itself did everything it was asked. The `systemdService` glyph's
content id had not moved, so the diff produced a `Noop` and the systemd
reconciler never ran. What *had* changed was a drop-in file (ADR 0041), and a
changed file under a unit directory fires ADR 0020's structural restart bracket
— `daemon-reload` then `systemctl try-restart`. `try-restart` on a unit that is
not running is a successful no-op: exit 0, nothing started. Every op was `Done`,
every op was honest, and the services were down. The same hole sits under ADR
0036's notify path, where the verb is `try-reload-or-restart`.

The `try-` verbs are there on purpose. ADR 0036 rejected `reload-or-restart`
because activation policy belongs to the `systemdService` glyph, not to a
notification side effect — a unit the author left inactive must stay inactive
when an unrelated config file moves. That is the right rule for an *inactive*
unit and it is not in question here.

Two facts decide how far past it this ADR may go.

**Neither of the two non-apply paths knows what the scroll declared.**
`foreman::propagate_config` derives the structural set from
`unit_for_config_file(changed path)` — the unit a changed file *names* by its
location — and the notify set from the author-listed `notifies` strings. Neither
consults the desired scroll's `systemdService` glyphs, and neither could without
a change of shape: `Reconciler::restart_unit` and
`Reconciler::try_reload_or_restart` receive a unit name and nothing else. So a
scroll carrying only a drop-in `file` glyph under `nginx.service.d/`, or a
`notifies nginx.service` naming a unit the host manages, reaches `nginx.service`
today with a `try-` verb — and would reach it with a forcing verb under any rule
phrased as "when the unit is failed". An argument for this change that leans on
"golem declared this unit" is therefore not available on the paths that
motivated it.

**`failed` is not a state an operator reaches by stopping something.**
`systemctl is-failed` asks exactly whether the unit's active state is `failed`
(`systemctl(1)`: "Check whether any of the specified units is in the 'failed'
state"). `systemctl stop` leaves a unit `inactive`, and a unit that was never
started is `inactive`. A unit is `failed` because a start job failed, because it
hit the start rate limit, or because it ran and exited in failure — in every one
of them something had already asked the unit to run, and that request is on
record in systemd itself. That is the distinguishing property, and it is
available to code that knows only a unit name.

The production units were quadlets, so the apply path's generated-unit fallback
(`enable` refused → `start`) is in scope too.

## Decision

**golem probes for the failed latch and clears it, and only then does the
forcing verb come out.** A new `HostReconciler::systemd_failed` runs `systemctl
is-failed <unit>` — exit 0 means failed — as a sibling of the existing
`is-enabled` / `is-active` probes. When it answers yes,
`HostReconciler::clear_failed_latch` runs `systemctl reset-failed <unit>` before
the start that follows, on all three paths:

- **`apply_systemd`** probes *after* the settled-unit early return, so a unit
  already enabled and active issues no extra command, and clears the latch
  before `daemon-reload` and `enable --now` (and therefore before the generated
  unit's `start` fallback). The recorded `Inverse::DisableSystemdService` is
  unchanged, `started_only` included.
- **the restart bracket (`try_restart`)** probes once, clears when latched, and
  picks its verb from `restart_verb(latched_failed)` — `restart` when latched,
  `try-restart` otherwise.
- **the notify path (`try_reload_or_restart`)** does the same via
  `reload_or_restart_verb(latched_failed)` — `reload-or-restart` when latched,
  `try-reload-or-restart` otherwise. It still issues no `daemon-reload`; ADR
  0036's reason for that absence is untouched.

**The `try-` verbs keep every case ADR 0036 gave them.** A merely inactive unit
is not failed, gets no `reset-failed`, and gets the `try-` verb exactly as
before. The only command this decision adds to a non-latched path is the
read-only `is-failed` probe; no mutating command changed for any unit that is
not latched failed.

**The gate is the unit's state, not the scroll's declaration, and that is an
expansion — stated here rather than implied.** On the restart bracket and the
notify path the forcing verb reaches whatever unit those paths already reach: a
unit named by the location of a config file golem wrote, or a unit named by an
author's `notifies`, *including a unit no `systemdService` glyph declares*. A
host-managed `nginx.service` that a golem-written drop-in refers to, or that a
`notifies` names, will now be reset and started if it is latched failed. What
holds the line is the meaning of `failed` rather than the contents of the
scroll: golem is restarting a unit that something already asked systemd to run
and that systemd could not keep running — not starting a unit nobody asked for,
which is the thing ADR 0036's rejection protects and which a merely inactive
unit still is.

The honest cost of that framing: an operator who *knows* a unit is broken and
leaves it latched, using the failed state as a brake, loses the brake — golem
will clear it and try. Gating the forcing verb on a declared `systemdService`
glyph was considered and not taken (Alternatives); this is the exposure that
choice accepts, and the mitigation is that nothing forces a unit no golem path
was already poking.

**A `reset-failed` that itself fails is best-effort — warn and proceed.**
`clear_failed_latch` returns `()`, not an `EnactResult<()>`, and no call site
`?`s it. Three reasons, in order of weight. The next command already classifies
the failure correctly: every caller immediately runs a start whose failure
becomes `EnactError::Retryable`, so if the latch really was the obstacle and
really was not cleared, the reconcile is `Retryable` a line later anyway —
making the reset itself `Retryable` adds no outcome and only removes them. And
it removes the right ones: the unit vanishing between probe and reset, a
transient D-Bus hiccup, a race with another `reset-failed` are all cases where
the start would have succeeded, so classifying the reset as `Retryable` turns a
working reconcile into a failed one — a worse failure mode than the bug being
fixed. Last, the operator wants the start's error, not the reset's: *"start
request repeated too quickly"* names the symptom, *"Failed to reset failed state
of unit …"* says a cleanup step did not land. The `warn` keeps that second-order
detail in the log, adjacent to the error that follows. A transport error from
the runner folds into the same warn.

**`reset-failed` is deliberately absent from the recorded `Inverse`.** This
touches golem's core invariant — reverse restores the prior state golem changed
— so the absence is a decision, not an omission. There is no inverse operation
to record: systemd has no `set-failed`, and an `Inverse` variant exists to be
enacted by `reverse`. The latch is not state golem edited or anyone authored;
it is systemd's own count of how many times a start job already failed, and
`reset-failed` zeroes a counter systemd keeps about its own past. Restoring it
would be self-contradictory: `reverse` runs `stop` or `disable --now`, and
re-latching a unit reverse has just stopped would leave the host worse than
golem found it, with the next operator-initiated start refused by a latch golem
re-armed. Finally the existing inverse is already exact — `prior_enabled` and
`prior_active` are observed before the reset, and `is-failed` and `is-active`
are disjoint answers, so a latched unit records `prior_active: false` either
way. The reset cannot corrupt the captured inverse; adding to it would.

## Consequences

- A unit systemd has latched is now recoverable by an ordinary golem apply. The
  fleet no longer needs a human running `systemctl reset-failed` before golem's
  own work can land.
- ADR 0036's boundary moves, and the part that moved is pinned by tests: a
  merely inactive unit still gets `try-restart` / `try-reload-or-restart`, no
  `reset-failed`, no bare `restart`, and stays inactive. What no longer holds
  unqualified is "activation policy belongs to the `systemdService` glyph" — for
  a failed unit, activation now follows systemd's own record that the unit was
  meant to be running, on paths that never see the glyph at all.
- **The blast radius is the set of units golem already pokes**, which is wider
  than the set it declares: units named by golem-written files under unit
  directories, and units named by `notifies`. Widening either of those sets —
  for instance a `notifies` on a unit golem has no other relationship with — now
  carries the power to restart it, and is worth authoring with that in mind.
- Every start path costs one extra read-only `systemctl is-failed`, except a
  fully settled apply, which the early return still short-circuits before the
  probe.
- `reverse` is unchanged and stays exact. golem still only ever undoes edits it
  recorded; the latch was never one of them.
- **A `Noop` still reaches nothing.** The probe runs only when a reconciler
  runs, and an op whose content id has not moved never enters one. A host whose
  manifest is unchanged and whose units are latched is untouched by this
  decision — the incident was caught only because a drop-in changed and fired
  the restart bracket. Whether golem should look for latched units outside a
  diff-driven op is open, and is a question about the diff, not about this verb.
- **Whether `is-failed` should gate anything else is open.** It answers a
  question the reconciler asks nowhere else today — reporting, `golemctl plan`
  and diagnosis all still work from `is-enabled` / `is-active`. Nothing here
  decides that it should not; nothing here decided that it should.
- The trait method is still named `try_reload_or_restart` though it can now
  issue the forcing verb. Renaming it ripples through `foreman.rs`,
  `reconciler.rs`, `fake_reconciler.rs` and four integration tests; the name is
  flagged as inaccurate and the rename left as a separate mechanical change.

## Alternatives considered

- **Leave the `try-` verbs alone and require an operator to run
  `reset-failed`.** Rejected: it is the incident. golem reports green while the
  service is down, and the recovery step is the one thing golem is for.
- **Use the forcing verb unconditionally.** Rejected — this is exactly ADR
  0036's rejected alternative, and its reason holds: a notification would then
  start a unit the author deliberately left inactive, which is the
  `systemdService` glyph's call.
- **Force only units the desired scroll declares with a `systemdService`
  glyph.** This is the rule that would keep ADR 0036's sentence true verbatim,
  and it was considered and not taken. It does not fit the seam:
  `Reconciler::restart_unit` and `Reconciler::try_reload_or_restart` take a unit
  name, so gating means threading the desired scroll's declared units through
  the trait — a signature change across the trait, its three implementations,
  and the foreman that drives them — to narrow a case whose distinguishing
  property (`failed`, not `inactive`) is already legible from the unit alone.
  The choice to gate on state rather than declaration was made deliberately, not
  by omission, and the exposure it accepts is written into the Decision above.
  Should a host-managed unit ever be force-started where it should not have
  been, this is the alternative to reopen, and the trait signature is the price.
- **Make a failed `reset-failed` `Retryable`.** Rejected above: it adds no
  outcome the following start does not already produce, converts benign races
  into hard stops, and buries the diagnosis behind the housekeeping.
- **Record the latch in the `Inverse` so `reverse` restores it.** Rejected
  above: there is no operation to enact it with, the latch is not golem's edit,
  and restoring it would re-arm a refusal against the next operator.
