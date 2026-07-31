# golem

A small-fleet declarative orchestrator. Author a fleet in **Emet**, a typed functional language that compiles to a binary, content-addressed manifest — one scroll per host, each a list of glyphs over four kinds (`aptPackage`, `systemdService`, `file`, `lineInFile`). Per-node agents ingest the manifest, diff their scroll by content id, and reconcile bare-metal Debian state through reversible reconcilers with journalled, surgical undo. Higher-level shapes (workloads, services, ingress, Podman quadlets) are Emet library abstractions that compile down to the four glyphs.

## Documentation

- **[sites/website/](sites/website/)** — public docs site (Astro Starlight). Three-tier deployment guide: Hello agent (M1) → One app + DB (M2) → Litour on a box (M2+M3). Concepts pages on the three-layer model, journal-before-mutate, and the trust model. `cd sites/website && bun install && bun run dev` to preview.
- **[QUICKSTART.md](QUICKSTART.md)** — install + apply walkthrough for the current flow: `emetc build` a fleet, then `golemctl apply`.
- **[docs/adr/](docs/adr/)** — the accepted design decisions for the current model, including the binary manifest (0012–0013), golemd's glyph reconciliation (0014), and the reversible reconcilers (0015).
- **[smoke-test/run.sh](smoke-test/run.sh)** — bash end-to-end exerciser: install caddy, push remove bundle, verify clean orphan sweep. Crash-injection cases via `GOLEM_CRASH_AFTER`.
- **`apps/golemd/tests/smoke_install_remove.rs`** — same end-to-end test inside a `debian:trixie + systemd` container, runnable from any Linux box with Docker. `cargo test -p golemd --test smoke_install_remove --release -- --ignored`.

## CI

The complete test-and-build gate is `nix flake check` (`flake.nix`): the whole Cargo workspace's tests, the `apps/fleet` harness tests, and all four binary builds — `golemd`/`golemctl`/`emetc`/`emet-lsp`, each portable static-musl (pkgsStatic; golemd/golemctl deploy onto Debian guests, and `golemd-static`/`golemctl-static` remain as aliases). Any machine with nix runs the entire gate with that one command; CI is a self-hosted box that runs it on every push and pushes the built store paths to a cachix cache, so other machines substitute instead of rebuilding (design decision: [docs/adr/](docs/adr/) 0035; cachix wiring: [docs/design/ci-cachix-nix.md](docs/design/ci-cachix-nix.md)). The website container is built separately (`nix build --impure .#website-container` over a bun-built `dist/`) and is **build-only** — image push is deferred until a registry exists.