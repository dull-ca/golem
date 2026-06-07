# Golem — project conventions

## The model

A per-node **bookkeeping** agent — it records, resolves, and journals; it
does **not** touch the host (no install, no signing, no multi-node, no
resource kinds beyond `packages`). Enforcement layers on top later.

Vocabulary (chosen deliberately — keep it consistent):

- **Blueprint** — what a user submits: `{ name, packages }`.
- **commission / decommission** — the request to add/replace, or remove, a blueprint.
- **build / teardown** — what golem *would* do in answer to a successful
  commission / decommission. Vocabulary only today; nothing runs.
- **State** — resolved view: `packages: { pkg → [blueprint names that want it] }`.
- **Revision** — append-only journal entry per change (`Init` / `Commission` /
  `Decommission`); embeds the State and the Actions.
- **Action** — `Install` / `Remove`. Recorded, never executed.

Source of truth: the Rust types in `crates/golem-types/`. User-facing
contract: `nickel/lib.ncl`. Build & run: `QUICKSTART.md`.

## The wire format is an implementation detail

The model above is the contract between `golemctl` and `golemd` — not the
bytes. JSON today (Nickel exports it); a binary, statically-typed format is
the plan of record. The model doesn't change, the serializer does. Don't
elevate JSON details (key order, base64) as the headline, and don't add
features that only make sense for hand-written JSON.

## Skill routing

Match a request to a skill → invoke it via the Skill tool. Don't run gstack
ceremony (telemetry, gbrain, decision briefs, codex gates) unless asked.
Auto mode is on; prefer action over planning.

## Git

Never use git unless explicitly asked — no commits, pushes, branches, or
resets. The user decides when to commit.

## Docs site

`docs/` still describes an older, richer model. Leave it alone — not there yet.
