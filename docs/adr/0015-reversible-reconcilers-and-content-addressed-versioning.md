# 0015-reversible-reconcilers-and-content-addressed-versioning

## Status

Accepted.

**As shipped (Phase 3):** the `Inverse` enum landed as `Nothing`,
`RemoveAptPackage`, `DisableSystemdService { prior_enabled, prior_active }`,
`RestoreFile`, `DeleteFile`, and `RemoveLineInFile` (`crates/golemd/src/journal.rs`).
`Nothing` is the receipt when golem did not change the host, so reverse is a
no-op. The `file` inverse stores prior contents **inline** and reads them as
UTF-8, so a non-UTF-8 prior file is a fatal error rather than a restorable
inverse — the [RATIFY 6] inline-first choice, with binary/out-of-line blobs
still deferred. The reconcilers are exercised via the fake `CommandRunner` and
tempfiles; the **end-to-end run against a real Debian box** (install → upgrade
→ decommission) is deferred to a later phase.

## Context

ADR 0014 makes golemd's job "enact the four glyphs of a content-addressed scroll
on a real host" behind a `Reconciler` port. The user's hard requirement on that
port is that every glyph be **content-addressed (versioned) and
isometric/reversible**: if version *v2* of a glyph was installed, there must
exist a complete *v2 uninstaller* — the exact inverse of what was applied —
usable both to **upgrade** (uninstall old CID, install new CID) and to **remove**
(uninstall, install nothing). golemd must record *what it actually did* precisely
enough to reverse it later.

The four glyphs (`emet/crates/emet/src/ir.rs`) are deliberately simple, but each
has a different reversal shape:

- `aptPackage { name }` — install a package; reversal removes it. But: *was the
  package already present before golem touched it?* Reversing must not remove a
  package the host had independently.
- `systemdService { unit }` — enable + start a unit; reversal disables + stops.
  Same "prior state" question: was it already enabled?
- `file { path, contents, mode }` — write a file; reversal must restore whatever
  was there before (prior contents+mode, or *absent* if golem created it).
- `lineInFile { path, line }` — ensure one line present; reversal removes exactly
  that line — but only if golem added it (the line may have pre-existed).

The through-line: **reversibility is not a property of the glyph alone; it is a
property of the (glyph, prior-host-state) pair captured at apply time.** A pure
"install nginx" cannot be inverted without knowing what "install nginx" changed.
This is why ADR 0014's journal must store a *reversal record* per applied glyph.

Content addressing (ADR 0012/0013) gives the versioning axis for free: a glyph's
identity for "did this change?" is its `content_id`. Two scrolls that produce the
same glyph bytes ⇒ same CID ⇒ no-op; a changed field ⇒ new CID ⇒ upgrade.

## Decision

### 1. The `Reconciler` port: apply produces the inverse

Model each glyph reconciler as a pair, where **apply returns the receipt needed
to reverse it** — reversibility is designed into the return type, not bolted on:

```
Outcome {                     // the reversal record, journalled per glyph
    op:         GlyphOp,       // what was requested (Install/Remove/Replace/Noop)
    cid:        ContentId,     // the glyph's content id (versioning axis)
    inverse:    Inverse,       // the captured prior state to restore on reverse
    changed:    bool,          // false ⇒ host already matched (idempotent no-op)
}

trait GlyphReconciler {
    // Bring the host to `glyph`; capture prior state so it can be undone.
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> Result<Outcome>;
    // Restore the prior state recorded in a previous Outcome (exact inverse).
    fn reverse(&self, outcome: &Outcome) -> Result<()>;
}
```

- **`apply` is idempotent** (the old `Builder` contract survives): if the host
  already matches, `changed = false`, `inverse` is the trivial "nothing to
  restore." Re-applying the same CID is a no-op.
- **`reverse(apply(x))` returns the host to its pre-apply state** — the isometry
  requirement, per glyph. `Inverse` carries *exactly and only* what apply
  changed: for a package, "golem installed it (so remove on reverse)" vs "it was
  already present (so leave it)"; for a file, the prior bytes+mode or a
  "did-not-exist" marker.
- The port speaks glyph vocabulary, not tool vocabulary (`lw:hexagonal`): callers
  see `apply`/`reverse` over `Glyph`, never `apt-get`.

### 2. Content-addressed upgrade / removal / no-op

golemd's per-glyph decision (the pure diff of ADR 0014 §3) is driven by CIDs, by
comparing the **desired scroll's glyphs** against the **last-applied Outcomes**
stored in the journal, keyed by `Glyph::key()` (the stable identity string —
`apt:<name>`, `file:<path>`, …) not by CID:

- key present in both, **same CID** → **Noop**.
- key present in both, **different CID** → **Replace**: `reverse` the old
  Outcome, then `apply` the new glyph. (Upgrade = uninstall old version, install
  new version — the user's stated upgrade path.)
- key only in desired → **Install**: `apply`.
- key only in last-applied → **Remove**: `reverse` the old Outcome, apply
  nothing. (Removal = the v-N uninstaller, exactly.)

Keying the *diff* by `Glyph::key()` and the *version* by `content_id` separates
"which resource" from "which version of it" cleanly — a file at `/etc/app.conf`
is one resource across versions; its CID says whether the desired contents
changed. `Replace` is ordered **reverse-then-apply** so the host is never left
with two versions half-present.

### 3. Where prior state lives: the journal, not the host

The `Inverse` for each applied glyph is captured at `apply` time and **stored in
the journal Revision** (ADR 0014 §4), because the host itself does not reliably
remember "golem installed this vs it was already here." The journal is golem's
memory of its own edits. This makes reversal a pure function of *recorded intent*
plus a small enact step, and keeps the reconcilers replay-safe across golemd
restarts. Storing `Inverse` also bounds what golem will ever undo: golem only
ever reverses things it recorded applying — it never removes a package or line it
did not add. (File contents can be large; the `Inverse` for `file` may store the
prior bytes inline or a content-addressed blob reference — an implementation
choice for Phase 3, [RATIFY] if inline-only is acceptable for the first cut.)

### 4. The four concrete reconcilers (design level)

- **`aptPackage { name }`.**
  - apply: query if installed (`dpkg-query`); if not, `apt-get install -y name`
    and record `Inverse::InstalledByUs`; if already present, `changed=false`,
    `Inverse::WasPresent` (reverse is a no-op).
  - reverse: if `InstalledByUs`, `apt-get remove -y name`; if `WasPresent`, do
    nothing.
- **`systemdService { unit }`.**
  - apply: record prior enabled/active state (`systemctl is-enabled/is-active`);
    `enable --now unit`. `Inverse` = the prior state.
  - reverse: restore prior state — if golem enabled it, `disable --now`; if it was
    already enabled/active, leave it.
- **`file { path, contents, mode }`.**
  - apply: read prior `(contents, mode)` or note absence; write the desired
    contents+mode atomically (temp file + rename). `Inverse` = prior bytes+mode,
    or `Absent`.
  - reverse: if `Absent`, delete the file; else restore prior bytes+mode.
- **`lineInFile { path, line }`.**
  - apply: if `line` already present, `changed=false`, `Inverse::LinePresent`
    (reverse no-op); else append it, `Inverse::LineAddedByUs` (record enough to
    remove exactly that line — e.g. the line text; reverse deletes the first/last
    matching occurrence golem added).
  - reverse: if `LineAddedByUs`, remove that line; if `LinePresent`, do nothing.
    Note the interaction with `file`: if a later `file` glyph rewrote the same
    path, ordering within a scroll matters — golem applies a scroll's glyphs in
    list order and reverses in the exact reverse order (a LIFO undo stack), which
    the journal's ordered `Outcome` list already encodes.

### 5. Idempotency, ordering, and all-or-nothing

- **Idempotency**: every `apply` first observes actual host state; re-running a
  scroll at the same CID changes nothing (`changed=false` throughout).
- **Ordering**: a scroll's glyph operations are applied in list order and any
  reversal (Replace/Remove, or a rollback) runs in **reverse order** — a LIFO
  undo stack recorded as the ordered `Outcome` list. This is what makes composite
  reversal (e.g. file + lineInFile on the same path) exact.
- **All-or-nothing per reconcile**: the surviving retry spine (ADR 0014) applies;
  if a glyph op fails fatally mid-scroll, golem reverses the Outcomes already
  applied *this reconcile* (rollback) and journals nothing — the node stays at its
  last good scroll. [RATIFY: rollback-on-partial-failure vs. journal-partial-and-
  resume; recommendation: rollback, matching the old all-or-nothing spine.]

## Alternatives considered

1. **Stateless reversal (recompute the inverse from the glyph alone).** Rejected:
   the inverse of "install nginx" depends on whether nginx pre-existed; a glyph
   carries no prior-state, so a stateless inverse would either clobber
   pre-existing host state or refuse to remove anything. Capturing `Inverse` at
   apply time is what makes reversal exact.
2. **Read prior state from the host at reverse time instead of journalling it.**
   Rejected: the host cannot answer "did golem add this line / install this
   package, or was it already here?" — the very distinction reversal needs. The
   journal is the only reliable record of golem's own edits.
3. **Version by whole-scroll CID only, not per-glyph.** Rejected: a one-line
   change to one file would re-apply every glyph in the scroll. Per-glyph CIDs
   (already produced upstream conceptually; the manifest's per-scroll CID plus
   per-glyph keying) give minimal, precise upgrades. (Per-scroll CID still gates
   "did anything on this host change at all?" as a fast path.)
4. **Snapshot the whole machine for reversal (filesystem/package snapshots).**
   Rejected as over-scoped: the four primitives are small and individually
   reversible; a machine-snapshot layer is a different, heavier tool and not what
   "isometric per glyph" asks for.
5. **Forward-only convergence, no reverse (Ansible-style).** Rejected: the
   requirement is explicit reversibility (a complete vN uninstaller). Forward-only
   cannot remove what a decommission must remove, nor cleanly downgrade.

## Consequences

- **The `Reconciler` port returns an `Outcome`/`Inverse` receipt**, and the
  journal Revision (ADR 0014 §4) stores the ordered `Outcome` list — this is the
  load-bearing addition that makes upgrade and removal exact.
- **golemd only ever reverses edits it recorded**, so it will never remove a
  package, line, or file it did not add — safe co-tenancy with host state the user
  manages by hand.
- **Each glyph reconciler is a small, independently testable unit** (`lw:solid`
  SRP): apply+reverse over one primitive, tested via the fake reconciler and,
  later, against a real Debian box. The `reverse(apply(x))` isometry is a property
  test per glyph.
- **`Replace` is reverse-then-apply and rollback is LIFO** — ordering is now part
  of the contract, driven by the journal's ordered Outcomes.
- **Open items flagged for Phase 3** ([RATIFY] in `PLAN.md`): large `file`
  `Inverse` storage (inline vs blob), and rollback-vs-resume on partial failure.
- **Cross-references:** implements the `Reconciler` port ADR 0014 introduced;
  consumes the content IDs of ADR 0013/0012; the glyph set it reconciles is fixed
  by ADR 0002/0009. Higher-level abstractions that compile to these four glyphs
  are authored in Emet per ADR 0016 — golemd never grows a fifth reconciler for
  them.

## Addendum: systemd apply reloads before enable

The `systemdService` reconciler runs `systemctl daemon-reload` before
`systemctl enable --now <unit>`. A freshly written unit file — whether golem
wrote it directly (a `file` glyph earlier in the same scroll) or a Podman
quadlet generated it — is invisible to systemd until a reload; without it,
`enable` fails on a unit systemd has never seen. Found running golem on a real
Debian box via the fleet harness (`apps/fleet/`).

The reverse path deliberately does not reload. Reverse never writes a unit
file, so the unit is already loaded; deleting a unit golem wrote is the `file`
glyph's own inverse, not something the systemd reverse needs to account for.

## Addendum: idempotent re-apply must preserve inverses across a Noop

The applied state holds, per still-present glyph, the inverse that removes it
(§1). A re-apply that changes nothing enacts a `Noop`, and a `Noop` carries
`Inverse::Nothing` — this reconcile captured nothing to undo. Storing that empty
inverse over the glyph's recorded state overwrites the real inverse captured at
its original `Install`. Recorded state and host then diverge permanently: golem
believes it has nothing to reverse, while the glyph is still present on the host
with no way to take it back.

So the enacted outcomes are post-processed before they are persisted: for a
`Noop` glyph, the prior recorded inverse (keyed by `Glyph::key()`) is carried
forward; `Install` and `Replace` keep their freshly captured inverse (a
`Replace`'s inverse is the new version's undo and must not be overwritten with
the old version's); `Remove` drops the glyph. See
`foreman::preserve_prior_inverses`.

Reproduced live via the registry dogfood: apply → re-apply → apply-empty left
the container running while golem recorded zero glyphs, because the re-apply had
clobbered the container's inverse with `Nothing` and the final empty scroll then
had nothing to reverse.

## Addendum: the apt reconciler may refresh the package list

The `aptPackage` reconciler runs `apt-get update` before an install. A fresh
Debian cloud image ships with an empty package list, so an install would fail to
resolve the package without a refresh first. The refresh is per-glyph and
idempotent — a single refresh per reconcile would be cheaper, but the stateless
per-glyph adapter has no reconcile-scoped hook to hang one on without threading
shared state through the `CommandRunner` port. See `reconcilers::apply_apt`.
