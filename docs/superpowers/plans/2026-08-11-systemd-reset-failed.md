# Plan: clear a latched-failed systemd unit before starting it

## Context

Three production sites went down. Their units hit `StartLimitBurst` while the
network was missing:

```
dull-yyc-dev.service: Scheduled restart job, restart counter is at 5.
dull-yyc-dev.service: Start request repeated too quickly.
Active: failed (Result: exit-code) since 16:28:52
```

A unit latched in `failed` by the start rate limiter refuses every subsequent
start job until `systemctl reset-failed` clears it. golem reported the reconcile
green while nothing started.

Why green: the `systemdService` glyph's content id was unchanged, so the diff
produced a `Noop` and `apply_systemd` never ran. What changed was a drop-in
file, whose restart bracket calls `Reconciler::restart_unit` ->
`HostReconciler::try_restart` -> `systemctl try-restart <unit>`. `try-restart`
on a unit that is not running is a successful no-op: exit 0, nothing started.
The same hazard lives in `try_reload_or_restart` (`systemctl
try-reload-or-restart`). And on the Install path, `apply_systemd` on a
failed-but-enabled unit runs `enable --now`, whose start job the latched rate
limiter refuses, so it fails rather than recovering.

## Global constraints

- Test-first: red before green. Tests assert the exact `systemctl` command
  sequences via the existing `FakeCommandRunner` log
  (`apps/golemd/src/host.rs`, `fake` module).
- No fifth glyph kind. This is reconciler behaviour, not vocabulary.
- Preserve golem's invariant: it only ever undoes edits it recorded.
- House style: the implementer writes **zero** comments and zero doc comments;
  a separate documentation pass owns all prose.
- Verification is `cargo test` (workspace), `cargo clippy --all-targets -- -D
  warnings`, `cargo fmt`. Green means the actual output was seen.
- Do not push. Commits only.

## Task 1 — the reconciler fix, test-first

Files: `apps/golemd/src/reconcilers.rs`, `apps/golemd/src/host.rs` (the `fake`
module only).

Required behaviour:

1. A `systemd_failed(unit)` probe, sibling to the existing
   `systemd_enabled`/`systemd_active` (`reconcilers.rs` ~284), running
   `systemctl is-failed <unit>`; exit 0 means failed.
2. `apply_systemd` (~226): when the unit is failed, run `systemctl reset-failed
   <unit>` **before** the `daemon-reload` / `enable --now`, so the latched rate
   limiter cannot refuse the start job.
3. `try_restart` (~357): when the unit is failed, `reset-failed` and then use
   `systemctl restart <unit>` — the non-`try` verb — so the unit actually comes
   back. When the unit is not failed, the command sequence is byte-for-byte
   what it is today (`daemon-reload` then `try-restart`), with no stray probe
   changing behaviour.
4. `try_reload_or_restart` (~532): same shape — failed means `reset-failed`
   then `systemctl reload-or-restart <unit>`; not failed keeps
   `try-reload-or-restart` exactly as today.

Two decisions this task must make and justify in its report (the wording is
then owned by the documentation task):

- **A `reset-failed` that itself fails.** Retryable, or best-effort (log and
  proceed to the start that follows)? Argue from what the following command
  does with the failure, and from which error message an operator needs to
  see. State the verdict plainly — it goes in the commit message.
- **The `Inverse`.** `reset-failed` clears host state golem did not create.
  Does it belong in the recorded `Inverse`? Argue the answer; do not leave it
  implicit. If the answer is no, the reasoning is a documentation-task input,
  not a silent omission.

Test coverage required (all four, and they must fail before the fix):

- a failed, enabled unit on the apply path is `reset-failed` and then started;
- a failed unit in the restart bracket is `reset-failed` and then really
  restarted (`restart`, not `try-restart`);
- a healthy unit's command sequence is unchanged — no stray `reset-failed`, and
  `try-restart` / `try-reload-or-restart` still the verbs used;
- the `enable --now` generated-unit (quadlet) fallback still works.

The `fake` host models systemctl for these tests; it will need a `failed` set,
`is-failed`, `reset-failed`, and whatever verbs the new paths issue
(`restart`, `reload-or-restart`, `try-reload-or-restart`). An unmodelled
command is a `Fatal` there by design — keep that property.

## Task 2 — prose

Every comment and doc comment for Task 1's code, matching the register of the
file's existing doc comments (dense, explanatory, arguing the *why*). Includes:
the module header if the restart bracket's contract is described there, the
`apply_systemd` doc comment, the `try_restart` / `try_reload_or_restart`
comments, the `fake` host's new arms, and the two arguments Task 1 settled
(reset-failed failure classification, and why `reset-failed` is absent from the
`Inverse`).

Also in scope: an ADR recording the decision, because ADR 0036 explicitly
rejected "`reload-or-restart` (start-if-inactive) instead of `try-`" — this
change is a narrow, failed-only exception to that rejection and the record must
say so. And a survey of whether `docs/` or `sites/website/` prose describing
the restart bracket now reads false, plus whether CHANGELOG.md needs an entry
(the repo generates it from commits via git-cliff, ADR 0055).
