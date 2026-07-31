# 0036 — `golemctl plan` dry-run and unit-level `notifies` reload-or-restart

## Status

Accepted 2026-07-31. Fulfils the future refinement ADR 0020 §5 explicitly
deferred (an authored notify edge beyond the structural config-file
heuristic). Implementation plan:
`docs/superpowers/plans/2026-07-31-plan-verb-and-notify-reload.md`.

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
  back by rollback wants its service reloaded onto the restored file.
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
  notification side effect.
- **Client-side plan (golemctl computes the diff).** Rejected: the diff
  needs the host's journal (prior outcomes), which lives with golemd; the
  daemon owning it keeps one code path (`reconcile::plan`) authoritative.
