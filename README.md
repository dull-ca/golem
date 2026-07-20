# golem

A small-fleet declarative orchestrator. Author a fleet in **Emet**, a typed functional language that compiles to a binary, content-addressed manifest — one scroll per host, each a list of glyphs over four kinds (`aptPackage`, `systemdService`, `file`, `lineInFile`). Per-node agents ingest the manifest, diff their scroll by content id, and reconcile bare-metal Debian state through reversible reconcilers with journalled, surgical undo. Higher-level shapes (workloads, services, ingress, Podman quadlets) are Emet library abstractions that compile down to the four glyphs.

## Documentation

- **[docs/](docs/)** — public docs site (Astro Starlight). Three-tier deployment guide: Hello agent (M1) → One app + DB (M2) → Litour on a box (M2+M3). Concepts pages on the three-layer model, journal-before-mutate, and the trust model. `cd docs && bun install && bun run dev` to preview.
- **[DESIGN.md](DESIGN.md)** — the original design doc for the earlier, richer model (Nickel input → translator → system primitives, the seven correctness commitments, milestone ladder M1–M5). Retained for history; the current model is the Emet/scroll/manifest one above and in `emet/docs/adr/`.
- **[QUICKSTART.md](QUICKSTART.md)** — install + apply walkthrough for the current flow: `emetc build` a fleet, then `golemctl apply`.
- **[REVIEW.md](REVIEW.md)** — the code review of the original scaffold that drove the M1 fixes (build-blockers, journal-before-mutate misimplementation, wire-format hazards, design landmines).
- **[smoke-test/run.sh](smoke-test/run.sh)** — bash end-to-end exerciser: install caddy, push remove bundle, verify clean orphan sweep. Crash-injection cases via `GOLEM_CRASH_AFTER`.
- **`crates/golemd/tests/smoke_install_remove.rs`** — same end-to-end test inside a `debian:trixie + systemd` container, runnable from any Linux box with Docker. `cargo test -p golemd --test smoke_install_remove --release -- --ignored`.