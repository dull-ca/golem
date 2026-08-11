# 0036 — `golemctl plan` dry-run and unit-level `notifies` reload-or-restart

## Status

Accepted 2026-07-31. Fulfils the future refinement ADR 0020 §5 explicitly
deferred (an authored notify edge beyond the structural config-file
heuristic). Implementation plan:
`docs/superpowers/plans/2026-07-31-plan-verb-and-notify-reload.md`.

Narrowed by
[ADR 0057](0057-clearing-a-latched-failure-before-starting-a-unit.md)
(2026-08-11): a unit systemd has latched `failed` is cleared with `systemctl
reset-failed` and then poked with the non-`try` verb. The rejected alternative
below still stands for the case it was about — a merely *inactive* unit is left
alone by the `try-` verbs. It does **not** still stand as written for a failed
one: the gate is the unit's state, not the scroll's declaration, so a
`notifies` naming a unit no `systemdService` glyph declares can now restart it
when it is latched. ADR 0057 argues why (`failed` means something already asked
the unit to run) and records what that exposes. Everything else here, the absent
`daemon-reload` included, is unchanged.

## Context

golemctl can apply a manifest and watch it enact, but cannot answer "what
would this do?" without doing it. The pure diff already exists inside golemd
(`reconcile::plan`: prior outcomes + desired scroll → ordered `GlyphOp`s,
no side effects); only enactment wraps it in writes. Separately, ADR 0020
gave golemd a structural heuristic — a changed config file under a
recognized unit directory triggers `daemon-reload` + `systemctl
try-restart` — and recorded that an Emet-expressed dependency ("this unit
reloads when that file changes") was deferred until the heuristic proved
too coarse. It has: services like nginx read config outside unit
directories, and a restart is the wrong tool when the service supports
reload. Constraints: the four reconciler-owned glyph kinds are a hard
boundary (root CLAUDE.md, ADR 0031); the wire format is postcard,
non-self-describing, so any struct field addition is a `format_version`
bump; per-glyph content ids drive the diff, so anything stored on a glyph
perturbs its cid.

## Decision

**Plan.** golemd gains read-only `POST /plan`: same body as `POST
/manifest`, runs decode → host-scroll select → WAL read → per-unit diff +
vanished-removes + predicted reload set, and returns ordered rows
(`unit_path`, `glyph_key`, action, content ids) plus summary counts — no
attempt opened, no journal writes, callable during a live reconcile
(response names the revision it diffed against). golemctl gains a `plan`
verb rendering the collapsed view: one line per (action × glyph kind)
group in execution order, members and contributing units listed, the
coalesced reload step last; colored for scanning, plain when stdout is not
a tty or `NO_COLOR` is set; `--json` returns the response verbatim. `plan`
exits 0 whether or not changes exist; a diff-signalling exit code is
deferred until something needs it.

**Notifies.** `Scroll` gains `notifies: Vec<String>` (systemd unit names)
beside `policy`; Emet exposes it as an optional `notifies` field on the
`scroll` constructor. Semantics: when any glyph op in or under that scroll
completes `Done` with `changed == true`, the listed units join the
end-of-apply reload set. Branch-level `notifies` union downward over all
descendant leaves — unlike policy's nearest-wins cascade, because reload
obligations accumulate rather than override. The set is enacted once,
deduplicated, at the existing end-of-apply seam (after the unit and
removes phases, before settle), via `systemctl try-reload-or-restart` —
reload where supported, restart otherwise, skip if inactive. The ADR 0020
structural heuristic is unchanged (unit-file edits need a true restart);
where both name the same unit, restart wins. Reload steps are journaled as
irreversible WAL brackets (re-driven on crash recovery) and become visible
in progress projections as a synthetic terminal `<reloads>` group, so the
live apply tree and the plan render the same final line. `format_version`
bumps 3 → 4.

## Consequences

- "What will this apply do?" is now answerable safely from any machine
  that can reach golemd, with output scaled for humans (grouped, ordered,
  colored) and machines (`--json`).
- Reload wiring is authored declaratively next to the unit it protects,
  survives on the scroll hash without touching glyph cids — editing a
  `notifies` list never forces a spurious file re-write — and the glyph
  kind count stays four: the reload is an *outcome*, not desired state.
- A rolled-back unit still contributes its notifications: a config flipped
  back by rollback wants its service reloaded onto the restored file. The
  rule is "any changed row in the unit, reversals included" — deliberately
  *not* a net-effect test, so it keeps a rolled-back `Done` that ADR 0020's
  structural restart set drops: the structural pass is about a *unit file*,
  whose write-then-reverse leaves systemd's view of the definitions exactly
  where it started, while the notify pass is about a service's *inputs*,
  which genuinely moved twice. An install undone by rollback therefore still
  pokes its unit — benign (`try-reload-or-restart` is a no-op on an inactive
  unit, a cheap reload otherwise) and not to be "fixed" by netting installs
  against their reversals.
- A failed reload or restart is a reported outcome, not a silent one: the
  coalesced set is enacted under a synthetic `<reloads>` unit that appears in
  the reconcile report, so a failure logs, lands as a `GlyphFailure`, and
  makes the reconcile `partial` — the service is still running its old
  configuration, which is not a settled apply. The live report and the
  reattach-rebuilt one carry that group identically.
- The v4 bump means emetc and golemd move in lockstep (v3 artifacts fail
  cleanly, per the format's design); glyph cids are untouched, so the
  first v4 apply is a no-op pass, not a replace storm.
- Reloads are not reversible and not undone by `reverse`; the journal
  records that honestly (`Inverse::Nothing`).
- Foreclosed for now: per-glyph notify edges (revisit only if unit-level
  granularity proves too coarse — the same escape hatch ADR 0020 left),
  and starting inactive units from a notification (an inactive unit's
  desired state belongs to the `systemdService` glyph).

## Alternatives considered

- **A fifth glyph kind ("reload unit X").** Rejected: a reload is not a
  state to converge on but a consequence of other changes; modeling it as
  desired state breaks the diff (it would always be "new") and the
  four-kind boundary.
- **`notifies` on the glyph.** Rejected: perturbs the glyph's content id,
  so rewiring a notification forces a Replace of the underlying resource;
  also multiplies the field across every glyph in a unit.
- **Leaf-only `notifies` (no branch union).** Rejected for authoring
  ergonomics: a branch grouping ten nginx config leaves should say
  `notifies nginx.service` once.
- **`reload-or-restart` (start-if-inactive) instead of `try-`.** Rejected:
  activation policy belongs to the `systemdService` glyph, not to a
  notification side effect. Amended by ADR 0057 for the failed case only: a unit
  that is *failed* rather than inactive is reset and forced, because the `try-`
  verb is a silent no-op against a latch and no glyph can express its way past
  one. That forcing is gated on the unit's state, so it can reach a notified
  unit this scroll never declared — the sentence above holds for inactive units,
  not for failed ones.
- **Client-side plan (golemctl computes the diff).** Rejected: the diff
  needs the host's journal (prior outcomes), which lives with golemd; the
  daemon owning it keeps one code path (`reconcile::plan`) authoritative.

## Addendum — the reload set subtracts units this reconcile removed (2026-07-31)

The coalesced reload set now drops any unit whose own definition this reconcile
took off the host, before anything is journaled.

The decision's rule — "any glyph op in or under that scroll completes `Done` with
`changed == true`" — is indifferent to *which way* the change went, and a
teardown is all changes. Decommissioning a scroll that notifies
`golem-nftables.service` removed the service's unit file and disabled the unit,
and then ran `systemctl try-reload-or-restart golem-nftables.service` against a
unit systemd no longer had. `Unit not found` is a failed reload, a failed reload
is a `GlyphFailure` under `<reloads>`, and a teardown that reversed every glyph
correctly reported `partial`. Found on the VM fleet applying the ADR 0041
nftables fixture and then an empty scroll.

A unit is "removed" when a `Done`, un-`Reversed` `Remove` enacted its
`systemdService` glyph or the file that *defines* it
(`foreman::removed_units`/`unit_defined_by`). A drop-in under `<unit>.service.d/`
is deliberately excluded: removing one leaves the unit standing with different
configuration, which is precisely the case that still wants a reload. The
subtraction is visible, not silent — each skip logs and records an `Info`
progress event under the `<reloads>` path — and `foreman::predicted_reloads`
applies the same rule over planned ops, so `golemctl plan` and the apply it
predicts still render the same final line.

**Deliberately not changed: a unit whose install was undone by
`on_exhaust = rollback` still gets poked.** The Consequences reason that a
rolled-back `Done` is a genuine second movement of a service's inputs and must
keep its notification — "benign (`try-reload-or-restart` is a no-op on an
inactive unit, a cheap reload otherwise) and not to be 'fixed' by netting
installs against their reversals." That reasoning assumes the unit file still
exists, so the poke lands on a real unit and does nothing. When the rollback
reversed the unit file itself, it does not: the poke fails `Unit not found`
exactly as the removal case did, and turns a rollback that behaved correctly into
a `partial` report. Removal and rollback are not the same case — a removal is the
authored outcome, a rollback is a failure being contained — so this addendum
declines to collapse them by reflex. Whether the reload set should also subtract
units a rollback un-defined is left open for a decision of its own.
