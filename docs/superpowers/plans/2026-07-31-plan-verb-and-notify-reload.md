# Plan: `golemctl plan` + notify/reload-or-restart

Two features, one ADR (0036), sequenced so the first ships without wire
changes and the second lands the one `format_version` bump.

## Feature A — `golemctl plan`: dry-run diff, collapsed and colored

### Server: `POST /plan` on golemd

A read-only sibling of `POST /manifest`, same body (raw manifest bytes),
reusing `ingest`'s pure prefix and none of its writes:

decode + `format_version` check → `select` host scroll → `wal_steps` →
`applied_outcomes` → per leaf unit `reconcile::plan` (Removes filtered, as
`run_reconcile` does) → `plan_vanished_removes` → predicted reload/restart
set (Feature B's rules; before Feature B lands, the existing ADR 0020
structural heuristic only). No `open_attempt`, no `progress.open`, no 409
gate — it can run while a reconcile is in flight (the WAL read is a
snapshot; response says which revision it diffed against).

Response (JSON), ordered exactly as execution would order it — units in
leaf order with ops in source order, the vanished-removes group last
(reverse prior order), then the coalesced reload step:

```json
{
  "host": "web-01",
  "scroll_content_id": "3f9c…",
  "against_revision": 12,
  "ops": [
    { "unit_path": ["web", "nginx"], "glyph_key": "file:/etc/nginx/nginx.conf",
      "action": "replace", "old_cid": "…", "new_cid": "…",
      "describe": "file /etc/nginx/nginx.conf (mode 644)" },
    …
  ],
  "reloads": [
    { "unit": "nginx.service", "kind": "reload-or-restart",
      "triggered_by": ["file:/etc/nginx/nginx.conf"] }
  ],
  "summary": { "install": 12, "replace": 3, "remove": 1, "noop": 42 }
}
```

`describe` reuses `Glyph::describe` (already documented as "the plan view,
not the wire contract"). `unit_path` + `glyph_key` mirror the progress wire
shape so future renderers can share code with the apply tree.

### Client: `golemctl plan <addr> <program.emet|manifest>`

Same front door as `apply`: shells to `emetc build` for `.emet` sources,
POSTs, renders. `--json` dumps the response verbatim.

**Rendering — the collapsed view.** Not the live per-task tree: group ops
by (action, glyph kind), one line per group, groups in first-occurrence
execution order; each line lists its members and, in parens, the leaf units
that contributed them. The reload step is always the last line. Mockup:

```
Plan for web-01 · against revision 12 · manifest 3f9c1a…

  + install   10 apt packages     nginx curl jq ripgrep …  (web/base, web/nginx)
  + install    2 systemd units    nginx.service golem-ci.timer  (web/nginx, ci)
  ~ replace    3 files            /etc/nginx/nginx.conf (web/nginx)
                                  /etc/motd (base)  /etc/hosts (base)
  - remove     1 line-in-file     /etc/hosts: "10.0.0.3 oldhost"
  ↻ reload     1 unit             nginx.service ← /etc/nginx/nginx.conf

  16 changes · 12 install, 3 replace, 1 remove · 42 unchanged
```

Color: green `+ install`, yellow `~ replace`, red `- remove`, cyan
`↻ reload`, dim counts/unchanged; glyph-kind column bold; unit provenance
dim. Long member lists wrap under their own column, capped with `… and N
more` past a threshold (full list always in `--json`). Unchanged glyphs are
a count only; `--detail` expands every group to one-glyph-per-line with
cids. Color off when stdout is not a tty or `NO_COLOR` is set (same policy
as the existing view; no auto-detect surprises in CI). Exit code 0; a
future `--exit-code` mode (terraform's `detailed-exitcode` idiom) can wait.

Implementation: a new `plan.rs` renderer beside `view.rs`, sharing its
styling idioms; snapshot tests via a bounded `render_to_string` like
`view::render_to_string_bounded`.

## Feature B — `notifies`: declarative reload-or-restart, coalesced

### The model (ADR 0036, the refinement ADR 0020 §5 promised)

- **`notifies` lives on the Scroll (unit), not the glyph.** New field
  `notifies: Vec<String>` (systemd unit names) beside `policy`. Semantics:
  if any glyph op in (or under) that scroll lands `Done` with
  `changed == true`, the listed units join the end-of-apply reload set.
  Branch-level `notifies` **unions** down to every leaf beneath (unlike
  policy's nearest-wins — reloads accumulate, they don't override).
- **Why not on the glyph:** policy already set the precedent that
  enactment metadata rides the scroll — inside the scroll hash but outside
  every glyph cid (`scroll.rs:88-93`). On the glyph, editing a `notifies`
  list would change the glyph's content id and force a spurious
  Replace — re-writing a file because its notification wiring changed.
  Unit-level avoids that, and the four reconciler-owned glyph kinds stay
  four.
- **Reload vs restart:** enacted via `systemctl try-reload-or-restart` —
  reload if the unit supports it, restart otherwise, nothing if inactive.
  That is exactly the requested semantic and it's one systemd command. The
  existing ADR 0020 structural heuristic (config file under a unit
  directory → `daemon-reload` + `try-restart`) stays as-is — unit-file
  changes need a real restart — and the two sets merge, deduped, restart
  winning over reload for the same unit.
- **Coalescing & ordering:** the seam already exists —
  `propagate_config` runs once after the unit phase and the removes phase,
  before `settle`, and already dedupes its restart list. `notifies`
  extends that collection; execution stays exactly-once-per-unit at the
  end of the apply.
- **Journal/visibility:** reload steps get WAL brackets keyed
  `reload:<unit>` (Inverse::Nothing — a reload is not reversible),
  re-driven idempotently on recovery like restarts. Today
  `WalAction::Restart` is excluded from the projection, so the TUI never
  shows the coalesced step; that exclusion is lifted and restart/reload
  steps join a synthetic `<reloads>` terminal group (the `<removes>`
  precedent), so **both** the live apply tree and the plan show the same
  final line. (Implementer must check WAL decode compatibility before
  adding a `WalAction` variant — the WAL is host-local but postcard-coded;
  if old journals would misparse, gate on the existing attempt/journal
  versioning or add the variant last.)

### Wire change

`format_version` 3 → 4 (field added to `Scroll`; postcard is
non-self-describing, so this is a bump by definition — the NOTEs in
`scroll.rs` say exactly this). Glyph cids are untouched, so the first apply
of a v4 manifest is a Noop pass, not a Replace storm. emetc and golemd move
in lockstep as always; a v3 artifact fails cleanly.

### Emet surface

`notifies` as an optional field on the `scroll` constructor, type
`List String`:

```emet
scroll
    { name = "nginx"
    , notifies = [ "nginx.service" ]
    , contents = [ file { path = "/etc/nginx/nginx.conf", … } ]
    }
```

Touch points mirror `retry`: `parser.rs` `build_constructor`, `ast.rs`,
`infer.rs` (unify with `List String`), `eval.rs` lowering, plus
`scroll-format` (`Scroll` field, determinism tests) and the corpus
(diagnostics for a mistyped `notifies` — wrong element type, unknown
field).

## What this does NOT do

- No fifth glyph kind. `notifies` is unit metadata; the reload is an
  outcome, not desired state.
- No per-glyph notify edges (revisit only if unit-level proves too coarse
  — same escape hatch ADR 0020 left).
- No reverse of a reload; rollback of a unit does re-trigger notification
  collection (a rolled-back config change that flipped a file back also
  wants the reload).

## Task breakdown (implementer/documenter pairs per repo workflow)

1. **golemd `POST /plan`** — endpoint, response types, tests (plan against
   empty WAL, against a prior revision, mid-reconcile snapshot). No wire
   bump.
2. **`golemctl plan`** — subcommand, renderer + color policy, snapshot
   tests, `--json`.
3. **scroll-format v4 + Emet `notifies`** — field, FORMAT_VERSION bump,
   parser/infer/eval, corpus diagnostics, determinism tests.
4. **golemd notify enactment** — collection from WAL `changed` rows +
   unioned scroll `notifies`, merge with structural heuristic,
   `try_reload_or_restart` on the Reconciler trait (+ fake), WAL brackets,
   projection un-exclusion + `<reloads>` group, fleet `reload-proof.emet`
   extension.
5. **Plan × notify integration** — predicted `reloads` in `/plan`,
   rendered line, end-to-end test.
6. **ADR 0036 + docs** — ADR (notes ADR 0020 §5 fulfilled), website
   reference pages (`plan` verb, `notifies` field), documenter comment
   passes over 1–5.

Sequencing: 1 → 2 ship alone (useful immediately); 3 → 4 → 5 carry the
bump; 6 closes. Review gate: whole-branch review after 5, before 6.

## Open questions folded into decisions (flag if you disagree)

- Branch `notifies` unions downward (vs leaf-only): chosen for authoring
  ergonomics — a `web/nginx` branch notifying `nginx.service` covers all
  its leaves.
- `try-reload-or-restart` (skip if inactive) vs `reload-or-restart`
  (start if inactive): chose `try-` — an inactive unit's desired state is
  the systemdService glyph's job, not the notifier's.
- Plan verb exit code always 0 (changes are not an error); `--exit-code`
  deferred.
