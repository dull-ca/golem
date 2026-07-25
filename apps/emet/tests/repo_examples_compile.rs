//! The compiler crate's own `examples/` are covered by `examples.rs`; this
//! suite guards the repo-level entry programs outside that directory — the
//! ones shipped under `apps/fleet/` and `examples/` — so a language change
//! that breaks them is caught here rather than by a human running each one
//! by hand.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn compiles(entry: &str) {
    let path = repo_root().join(entry);
    match emet::compile_file(&path) {
        Ok(_) => {}
        Err(e) => panic!("{entry} failed to compile at {:?}: {}", e.phase, e.msg),
    }
}

#[test]
fn fleet_smoke_programs_compile() {
    compiles("apps/fleet/smoke.emet");
    compiles("apps/fleet/reload-proof.emet");
}

#[test]
fn example_fleets_compile() {
    compiles("examples/lichess/fleet.emet");
    compiles("examples/registry/registry.emet");
    compiles("examples/registry/clients.emet");
    compiles("examples/website/website.emet");
    compiles("examples/website/builder.emet");
}
