# golem

A small-fleet declarative orchestrator. Write your services and how they're exposed in Nickel — `app` describes what runs and how services wire to each other, `deploy` says where it lives. Per-node agents reconcile bare-metal Debian state (packages, files, systemd units, Podman quadlets) with refcounted ownership and surgical undo.

## Documentation

- **[docs/](docs/)** — public docs site (Astro Starlight). Three-tier deployment guide: Hello agent (M1) → One app + DB (M2) → Litour on a box (M2+M3). Concepts pages on the three-layer model, journal-before-mutate, and the trust model. `cd docs && bun install && bun run dev` to preview.
- **[DESIGN.md](DESIGN.md)** — canonical design doc. Three-layer model (Nickel input → translator → system primitives), the seven correctness commitments, the corrected Provider trait that makes journal-before-mutate honest, milestone ladder M1–M5.
- **[QUICKSTART.md](QUICKSTART.md)** — install + apply walkthrough. Reflects the M2+ Nickel-driven flow; M1 today uses hand-written JSON bundles.
- **[REVIEW.md](REVIEW.md)** — the code review of the original scaffold that drove the M1 fixes (build-blockers, journal-before-mutate misimplementation, wire-format hazards, design landmines).
- **[smoke-test/run.sh](smoke-test/run.sh)** — bash end-to-end exerciser: install caddy, push remove bundle, verify clean orphan sweep. Crash-injection cases via `GOLEM_CRASH_AFTER`.
- **`crates/golemd/tests/smoke_install_remove.rs`** — same end-to-end test inside a `debian:trixie + systemd` container, runnable from any Linux box with Docker. `cargo test -p golemd --test smoke_install_remove --release -- --ignored`.