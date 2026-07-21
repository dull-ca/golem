# golem

A small-fleet declarative orchestrator. Author a fleet in **Emet**, a typed functional language that compiles to a binary, content-addressed manifest — one scroll per host, each a list of glyphs over four kinds (`aptPackage`, `systemdService`, `file`, `lineInFile`). Per-node agents ingest the manifest, diff their scroll by content id, and reconcile bare-metal Debian state through reversible reconcilers with journalled, surgical undo. Higher-level shapes (workloads, services, ingress, Podman quadlets) are Emet library abstractions that compile down to the four glyphs.

## Documentation

- **[sites/website/](sites/website/)** — public docs site (Astro Starlight). Three-tier deployment guide: Hello agent (M1) → One app + DB (M2) → Litour on a box (M2+M3). Concepts pages on the three-layer model, journal-before-mutate, and the trust model. `cd sites/website && bun install && bun run dev` to preview.
- **[QUICKSTART.md](QUICKSTART.md)** — install + apply walkthrough for the current flow: `emetc build` a fleet, then `golemctl apply`.
- **[docs/adr/](docs/adr/)** — the accepted design decisions for the current model, including the binary manifest (0012–0013), golemd's glyph reconciliation (0014), and the reversible reconcilers (0015).
- **[smoke-test/run.sh](smoke-test/run.sh)** — bash end-to-end exerciser: install caddy, push remove bundle, verify clean orphan sweep. Crash-injection cases via `GOLEM_CRASH_AFTER`.
- **`apps/golemd/tests/smoke_install_remove.rs`** — same end-to-end test inside a `debian:trixie + systemd` container, runnable from any Linux box with Docker. `cargo test -p golemd --test smoke_install_remove --release -- --ignored`.

## CI

CI runs on Codeberg via [Woodpecker](https://woodpecker-ci.org/), driven entirely by the nix toolchain (`.woodpecker.yml`, `flake.nix`). Every push and pull request: tests the whole Cargo workspace (`cargo test --workspace`), builds the `golemd`/`golemctl`/`emetc` release binaries as flake outputs, builds the static docs site with bun, and builds the Caddy website container to prove it packages. The container is **build-only** — image push is deferred until a registry exists.

By default the runners have no cache, so cache.nixos.org substitutes every nixpkgs dependency and only golem's own crates compile each run. For a real speedup, set a `CACHIX_AUTH_TOKEN` secret in the Codeberg repo CI settings and follow the cachix wiring at the bottom of `.woodpecker.yml` — a cachix cache then persists golem's built store paths across runs.