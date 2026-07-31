use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use golemctl::fleet::{self, Fanout, HostOutcome, HostPlan};
use golemctl::inventory::{self, Target};
use golemd::config::RetryConfig;
use golemd::fake_reconciler::FakeReconciler;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::planroom::SqlitePlanRoom;
use scroll_format::{Contents, Glyph, Manifest, Scroll};

const FLEET: [&str; 3] = ["h1", "h2", "h3"];

fn scroll_of(host: &str) -> Scroll {
    Scroll {
        name: host.to_string(),
        policy: None,
        notifies: vec![],
        contents: Contents::Glyphs(vec![Glyph::AptPackage {
            name: format!("{host}-pkg"),
        }]),
    }
}

fn manifest_bytes() -> Vec<u8> {
    manifest_naming(&FLEET)
}

fn manifest_naming(hosts: &[&str]) -> Vec<u8> {
    let scrolls = hosts.iter().map(|host| scroll_of(host)).collect();
    scroll_format::to_bytes(&Manifest::from_scrolls(scrolls, "fleet-fanout-test"))
}

fn content_ids() -> BTreeMap<String, String> {
    FLEET
        .iter()
        .map(|host| {
            (
                host.to_string(),
                scroll_format::content_id(&scroll_of(host)).to_string(),
            )
        })
        .collect()
}

// One in-process golemd: its own plan room in the tempdir, the fake
// reconciler, and the real router on an ephemeral port — the shape golemd's own
// integration tests use, so fan-out is exercised over real HTTP with no spawned
// binary and no host touched.
async fn serve(host: &str, state_dir: &Path) -> String {
    let room = SqlitePlanRoom::open(&state_dir.join(format!("{host}.db"))).unwrap();
    let foreman = Foreman::new(
        host.to_string(),
        Box::new(room),
        Box::new(FakeReconciler::new()),
    )
    .with_retry_config(RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        ..Default::default()
    });
    let app = http::router(http::AppState {
        foreman: Arc::new(foreman),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

// An address nothing listens on — bind, learn the port, drop the listener. A
// daemon that is down, refusing immediately rather than making the test wait
// out a timeout.
async fn closed_port() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}")
}

fn write_inventory(dir: &Path, addrs: &[(&str, String)]) -> Vec<Target> {
    let mut text = String::from("[hosts]\n");
    for (name, addr) in addrs {
        text.push_str(&format!("{name} = \"{addr}\"\n"));
    }
    let path = dir.join("fleet.toml");
    std::fs::write(&path, text).unwrap();
    inventory::load(&path).unwrap().select(None).unwrap()
}

async fn get_json(addr: &str, path: &str) -> serde_json::Value {
    reqwest::get(format!("{addr}/{path}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// Three in-process golemds behind one inventory: a fan-out must settle all of
/// them, and each must journal the content id of *its own* scroll — the proof
/// that fan-out changes nothing about scroll selection, which stays the
/// daemon's (ADR 0038).
#[tokio::test]
async fn a_fleet_apply_settles_every_host_and_each_daemon_records_its_own_content_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut addrs = Vec::new();
    for host in FLEET {
        addrs.push((host, serve(host, dir.path()).await));
    }
    let targets = write_inventory(dir.path(), &addrs);
    assert_eq!(targets.len(), 3);

    let fanout = Fanout::read(&manifest_bytes(), targets).unwrap();
    let results = fleet::apply_plain(manifest_bytes(), &fanout, false).await;

    assert_eq!(results.len(), 3);
    for (target, outcome) in &results {
        assert!(
            outcome.is_settled(),
            "{} did not settle: {outcome:?}",
            target.name
        );
    }
    assert_eq!(fleet::fleet_exit_code(&results), 0);

    let expected = content_ids();
    for (host, addr) in &addrs {
        let state = get_json(addr, "state").await;
        assert_eq!(
            state["content_id"].as_str().unwrap(),
            expected[*host],
            "{host} applied a different scroll"
        );
        assert_eq!(state["scroll"]["name"], *host);
    }
}

/// Per-host isolation: the middle host's address is a closed port, so its POST
/// cannot land. Its neighbours must still settle and still be reported, while
/// the aggregate exit turns 1 — one unreachable daemon costs the fleet its
/// zero, never the other hosts their apply.
#[tokio::test]
async fn a_downed_daemon_errors_alone_while_its_peers_settle_and_the_fleet_fails() {
    let dir = tempfile::tempdir().unwrap();
    let addrs = vec![
        ("h1", serve("h1", dir.path()).await),
        ("h2", closed_port().await),
        ("h3", serve("h3", dir.path()).await),
    ];
    let targets = write_inventory(dir.path(), &addrs);

    let fanout = Fanout::read(&manifest_bytes(), targets).unwrap();
    let results = fleet::apply_plain(manifest_bytes(), &fanout, false).await;

    let by_name: BTreeMap<&str, &HostOutcome> = results
        .iter()
        .map(|(target, outcome)| (target.name.as_str(), outcome))
        .collect();
    assert!(by_name["h1"].is_settled(), "{:?}", by_name["h1"]);
    assert!(by_name["h3"].is_settled(), "{:?}", by_name["h3"]);
    assert!(
        matches!(by_name["h2"], HostOutcome::Error { .. }),
        "{:?}",
        by_name["h2"]
    );
    assert_eq!(fleet::fleet_exit_code(&results), 1);

    let aggregate = fleet::apply_json(&results);
    assert_eq!(aggregate["hosts"]["h1"]["outcome"], "settled");
    assert_eq!(aggregate["hosts"]["h3"]["outcome"], "settled");
    assert!(aggregate["hosts"]["h2"]["error"].is_string());

    let summary = fleet::summary_lines(&results, false);
    let h1 = summary.iter().position(|line| line.starts_with("h1  http"));
    let h2 = summary.iter().position(|line| line.starts_with("h2  http"));
    assert!(summary[h1.expect("h1 has a heading") + 1].starts_with("  apply settled"));
    assert_eq!(
        summary[h2.expect("h2 has a heading") + 1]
            .split_whitespace()
            .next(),
        Some("error:")
    );
}

/// A fleet plan is a read: every host answers with its own diff, and each
/// daemon's journal must still hold nothing but the `init` revision it booted
/// with.
#[tokio::test]
async fn a_fleet_plan_reports_every_host_and_journals_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut addrs = Vec::new();
    for host in FLEET {
        addrs.push((host, serve(host, dir.path()).await));
    }
    let targets = write_inventory(dir.path(), &addrs);

    let fanout = Fanout::read(&manifest_bytes(), targets).unwrap();
    let results = fleet::gather_plans(manifest_bytes(), &fanout).await;

    assert_eq!(results.len(), 3);
    let aggregate = fleet::plan_json(&results);
    let expected = content_ids();
    for host in FLEET {
        assert_eq!(aggregate["hosts"][host]["host"], host);
        assert_eq!(
            aggregate["hosts"][host]["scroll_content_id"],
            expected[host]
        );
        assert_eq!(aggregate["hosts"][host]["summary"]["install"], 1);
    }

    let lines = fleet::plan_lines(&results, &Default::default());
    for host in FLEET {
        let heading = lines
            .iter()
            .position(|line| line.starts_with(&format!("{host}  http://")))
            .unwrap_or_else(|| panic!("{host} has a heading: {lines:?}"));
        assert_eq!(
            lines[heading + 1],
            "  against revision 1 · manifest ".to_string()
                + &expected[host].chars().take(6).collect::<String>()
                + "…",
            "{lines:?}"
        );
    }

    for (host, addr) in &addrs {
        let revisions = get_json(addr, "revisions").await;
        let journalled = revisions.as_array().unwrap();
        assert_eq!(
            journalled.len(),
            1,
            "{host} journalled a revision for a plan"
        );
        assert_eq!(journalled[0]["kind"], "init");
        assert!(get_json(addr, "state").await["content_id"].is_null());
    }
}

/// Absence is silence (ADR 0038): a manifest naming only two of the three
/// inventory hosts must leave the third's state and journal exactly as they
/// were. Were it POSTed to, golemd would resolve its absent scroll to the
/// empty one and decommission the box; instead it is reported skipped, in the
/// summary, the aggregate, and the plan — and the fleet still exits 0.
#[tokio::test]
async fn a_host_the_manifest_names_no_scroll_for_is_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut addrs = Vec::new();
    for host in FLEET {
        addrs.push((host, serve(host, dir.path()).await));
    }
    let targets = write_inventory(dir.path(), &addrs);
    let bytes = manifest_naming(&["h1", "h2"]);

    let fanout = Fanout::read(&bytes, targets).unwrap();
    let results = fleet::apply_plain(bytes.clone(), &fanout, false).await;

    assert_eq!(results.len(), 3);
    let by_name: BTreeMap<&str, &HostOutcome> = results
        .iter()
        .map(|(target, outcome)| (target.name.as_str(), outcome))
        .collect();
    assert!(by_name["h1"].is_settled(), "{:?}", by_name["h1"]);
    assert!(by_name["h2"].is_settled(), "{:?}", by_name["h2"]);
    assert_eq!(by_name["h3"], &HostOutcome::Skipped);
    assert_eq!(fleet::fleet_exit_code(&results), 0);

    let aggregate = fleet::apply_json(&results);
    assert_eq!(aggregate["hosts"]["h3"]["skipped"], true);
    let summary = fleet::summary_lines(&results, false);
    let h3 = summary
        .iter()
        .position(|line| line.starts_with("h3  http://"))
        .expect("h3 has a heading");
    assert_eq!(summary[h3 + 1], "  skipped — no scroll in manifest");

    let expected = content_ids();
    for (host, addr) in &addrs {
        let state = get_json(addr, "state").await;
        if *host == "h3" {
            assert!(
                state["content_id"].is_null(),
                "h3 was applied to despite carrying no scroll: {state}"
            );
            let revisions = get_json(addr, "revisions").await;
            assert_eq!(revisions.as_array().unwrap().len(), 1);
        } else {
            assert_eq!(state["content_id"].as_str().unwrap(), expected[*host]);
        }
    }

    let plans = fleet::gather_plans(bytes, &fanout).await;
    let skipped = plans
        .iter()
        .find(|(target, _)| target.name == "h3")
        .expect("h3 is still reported");
    assert_eq!(skipped.1, HostPlan::Skipped);
    let plan_lines = fleet::plan_lines(&plans, &Default::default());
    let h3 = plan_lines
        .iter()
        .position(|line| line.starts_with("h3  http://"))
        .expect("h3 is still reported");
    assert_eq!(plan_lines[h3 + 1], "  skipped — no scroll in manifest");
    assert!(!plan_lines.iter().any(|line| line.contains("Plan for")));
}

/// Status is an observation, not an assertion: a dead host reports
/// `unreachable` on its own line and in the aggregate, beside the live hosts'
/// daemon id, revision, and applied content id.
#[tokio::test]
async fn a_fleet_status_reads_every_host_reachable_or_not() {
    let dir = tempfile::tempdir().unwrap();
    let addrs = vec![
        ("h1", serve("h1", dir.path()).await),
        ("h2", serve("h2", dir.path()).await),
        ("h3", closed_port().await),
    ];
    let targets = write_inventory(dir.path(), &addrs);

    let readings = fleet::gather_status(&targets).await;
    assert_eq!(readings.len(), 3);

    let lines = fleet::status_lines(&readings, false);
    assert_eq!(lines[0], "· h1  rev 1  nothing applied", "{lines:?}");
    assert_eq!(lines[1], "· h2  rev 1  nothing applied", "{lines:?}");
    assert!(lines[2].starts_with("✗ h3  unreachable:"), "{lines:?}");

    let aggregate = fleet::status_json(&readings);
    assert_eq!(aggregate["hosts"]["h1"]["host"], "h1");
    assert_eq!(aggregate["hosts"]["h1"]["latest_revision"], 1);
    assert!(aggregate["hosts"]["h1"]["content_id"].is_null());
    assert!(aggregate["hosts"]["h3"]["error"].is_string());
}
