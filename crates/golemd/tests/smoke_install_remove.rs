//! Integration test: clean install + remove cycle inside a systemd-trixie
//! container. Mirrors Test 1 of `smoke-test/run.sh`.
//!
//! Prerequisites:
//!   1. `cargo build --release -p golemd -p golemctl`
//!   2. `docker build -t golem-smoke:trixie crates/golemd/tests/fixtures`
//!
//! Run:
//!   cargo test -p golemd --test smoke_install_remove -- --ignored --nocapture

#[path = "common/mod.rs"]
mod common;

use std::time::Duration;

use common::{
    host_keygen, host_sign, post_bundle, smoke_test_dir, AgentHandle, Harness,
};

const TICK_PERIOD_SECS: u64 = 5;

#[test]
#[ignore]
fn install_then_remove_cycle() {
    // ── Setup keys + signed bundles on the host ────────────────────────
    let workdir = tempfile::tempdir().expect("workdir");
    let key_stem = workdir.path().join("operator");
    host_keygen(&key_stem);
    let sk = workdir.path().join("operator.sk");
    let pk = workdir.path().join("operator.pk");
    assert!(sk.exists() && pk.exists());

    let bundle_v1 = smoke_test_dir().join("bundle-v1-install.json");
    let bundle_v2 = smoke_test_dir().join("bundle-v2-remove.json");
    assert!(bundle_v1.exists(), "missing {}", bundle_v1.display());
    assert!(bundle_v2.exists(), "missing {}", bundle_v2.display());

    let signed_v1 = host_sign(&bundle_v1, &sk);
    let signed_v2 = host_sign(&bundle_v2, &sk);
    let signed_v1_path = workdir.path().join("signed-v1.json");
    std::fs::write(&signed_v1_path, &signed_v1).unwrap();

    // ── Bring up container, stage configs, start agent ─────────────────
    let h = Harness::start();
    h.exec(&["mkdir", "-p", "/etc/golem", "/var/lib/golem"]);
    h.copy_in(&pk, "/etc/golem/trusted-keys");
    h.copy_in(&signed_v1_path, "/etc/golem/bundle.json");

    let agent = h.spawn_agent(&[]);
    h.wait_for_agent_ready();

    // ── Wait up to 4 ticks for caddy to converge ───────────────────────
    let deadline = std::time::Instant::now()
        + Duration::from_secs(TICK_PERIOD_SECS * 8);
    let mut converged = false;
    while std::time::Instant::now() < deadline {
        let (code, _) =
            h.exec(&["systemctl", "is-active", "--quiet", "caddy.service"]);
        if code == 0 {
            converged = true;
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    if !converged {
        eprintln!("agent log:\n{}", h.dump_log());
        panic!("caddy never became active");
    }

    // ── Verify install: file present with the right content, package installed
    let (code, out) = h.exec(&["cat", "/etc/caddy/Caddyfile"]);
    assert_eq!(code, 0, "Caddyfile missing");
    assert!(
        out.contains("hello from golem"),
        "Caddyfile content wrong:\n{out}"
    );
    let (code, status) = h.exec(&[
        "dpkg-query", "-W", "-f=${Status}", "caddy",
    ]);
    assert_eq!(code, 0, "dpkg-query for caddy failed");
    assert!(
        status.contains("install ok installed"),
        "caddy not installed: {status}"
    );

    // ── Push remove bundle, expect orphan sweep ────────────────────────
    let url = h.agent_url();
    post_bundle(&url, &signed_v2).expect("push remove bundle");

    // Wait for full convergence: orphan sweep finished AND the journal is
    // empty AND the OS-side state matches. Polling on journal-empty as the
    // primary signal avoids the race where `dpkg-query` reports caddy gone
    // before `apt-get autoremove` returns and the reconciler calls forget().
    let deadline = std::time::Instant::now()
        + Duration::from_secs(TICK_PERIOD_SECS * 16);
    let mut swept = false;
    while std::time::Instant::now() < deadline {
        let (sql_code, rows) = h.exec(&[
            "sqlite3",
            "/var/lib/golem/state.db",
            "SELECT count(*) FROM claim_state;",
        ]);
        if sql_code == 0 && rows.trim() == "0" {
            swept = true;
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    if !swept {
        eprintln!(
            "journal contents:\n{}",
            h.exec(&[
                "sqlite3",
                "/var/lib/golem/state.db",
                "SELECT id_kind, id_key FROM claim_state;",
            ])
            .1
        );
        eprintln!("agent log:\n{}", h.dump_log());
        panic!("orphan sweep did not converge to empty journal");
    }

    // OS-side: caddy must actually be gone after the sweep.
    let (caddy_code, _) =
        h.exec(&["systemctl", "is-active", "--quiet", "caddy.service"]);
    assert_ne!(caddy_code, 0, "caddy.service still active after sweep");
    let (file_code, _) = h.exec(&["test", "-f", "/etc/caddy/Caddyfile"]);
    assert_ne!(file_code, 0, "/etc/caddy/Caddyfile still present after sweep");
    let (_, pkg_out) =
        h.exec(&["dpkg-query", "-W", "-f=${Status}", "caddy"]);
    assert!(
        !pkg_out.contains("install ok installed"),
        "caddy package still installed after sweep: {pkg_out}"
    );

    // ── Cleanup ────────────────────────────────────────────────────────
    h.kill_agent(&agent);
    let _ = AgentHandle { pid: agent.pid };
}
