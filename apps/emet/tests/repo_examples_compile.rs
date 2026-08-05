//! The compiler crate's own `examples/` are covered by `examples.rs`; this
//! suite guards the repo-level entry programs outside that directory — the
//! ones shipped under `apps/fleet/` and `examples/` — so a language change
//! that breaks them is caught here rather than by a human running each one
//! by hand.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
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
    compiles("apps/fleet/notify-proof.emet");
}

#[test]
fn example_fleets_compile() {
    compiles("examples/fishnet-farm/farm.emet");
    compiles("examples/lichess/fleet.emet");
    compiles("examples/registry/registry.emet");
    compiles("examples/registry/clients.emet");
    compiles("examples/website/website.emet");
    compiles("examples/website/builder.emet");
}

const TEST_FLEET_KEY: &str = "00112233445566778899aabbccddeeff\
                              00112233445566778899aabbccddeeff\
                              ffeeddccbbaa99887766554433221100\
                              ffeeddccbbaa99887766554433221100";

/// `examples/limesurvey/` calls `Secretspec.get`, so it cannot go through
/// `compiles` — resolving a secret needs a provider and a fleet key, and
/// `compile_file` supplies neither (ADR 0047: `emetc` is not hermetic for a
/// program that uses a secret).
///
/// The `env` provider is what keeps this in CI at all. `dotenv` would need a
/// `.env` beside the example's `secretspec.toml`, which is gitignored precisely
/// so resolved secret values never land in the tree. Process environment needs
/// no file, so the values live here, visibly fake, and the key is the same
/// throwaway one `secrets.rs` uses.
#[test]
fn the_limesurvey_example_compiles_with_a_provider_and_a_key() {
    let key_file = std::env::temp_dir().join(format!(
        "emet_limesurvey_key_{}.hex",
        std::process::id()
    ));
    std::fs::write(&key_file, TEST_FLEET_KEY).expect("write the throwaway fleet key");

    std::env::set_var("LIMESURVEY_ADMIN_PASSWORD", "not-a-real-admin-password");
    std::env::set_var("LIMESURVEY_DB_PASSWORD", "not-a-real-database-password");

    let entry = repo_root().join("examples/limesurvey/main.emet");
    let outcome = emet::compile_file_with(
        &entry,
        emet::secrets::SecretOptions {
            key_file: Some(key_file.clone()),
            provider: Some("env".to_string()),
            profile: Some("default".to_string()),
        },
    );
    let _ = std::fs::remove_file(&key_file);

    if let Err(e) = outcome {
        panic!(
            "examples/limesurvey/main.emet failed to compile at {:?}: {}",
            e.phase, e.msg
        );
    }
}
