use std::sync::Arc;
use std::time::Duration;

use golemctl::conn::{AuthSource, Conn};
use golemctl::inventory::{Endpoint, Target};
use golemctl::plan::{self, Action, Observed, PlanResponse, RenderOptions};
use golemd::config::RetryConfig;
use golemd::fake_reconciler::FakeReconciler;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::planroom::MemoryPlanRoom;
use golemd::secrets::Keyring;
use scroll_format::{Chunk, Contents, Entry, Glyph, Manifest, Perms, Scroll, Secret, Text};

fn perms() -> Perms {
    Perms {
        mode: 0o644,
        owner: None,
        group: None,
    }
}

fn file_glyph(path: &str, contents: &str) -> Glyph {
    Glyph::Filesystem {
        path: path.to_string(),
        entry: Entry::File {
            contents: contents.into(),
            perms: perms(),
        },
    }
}

fn sealed_file_glyph(path: &str) -> Glyph {
    Glyph::Filesystem {
        path: path.to_string(),
        entry: Entry::File {
            contents: Text::composed(vec![Chunk::Hole(Secret::Sealed {
                key_id: "6fb6c6005355abf3".to_string(),
                ciphertext: vec![0; 32],
            })]),
            perms: Perms {
                mode: 0o600,
                owner: None,
                group: None,
            },
        },
    }
}

fn manifest_of(host: &str, glyphs: Vec<Glyph>) -> Vec<u8> {
    let scroll = Scroll {
        name: host.to_string(),
        policy: None,
        notifies: vec![],
        contents: Contents::Glyphs(glyphs),
    };
    scroll_format::to_bytes(&Manifest::from_scrolls(
        vec![scroll],
        "plan-against-host-test",
    ))
}

fn http_target(name: &str, url: String) -> Target {
    Target {
        name: name.into(),
        endpoint: Endpoint::Http { url },
        token_file: None,
    }
}

async fn serve_sharing_journal(
    host: &str,
    room: Arc<MemoryPlanRoom>,
    reconciler: FakeReconciler,
) -> String {
    let foreman = Foreman::new(host.to_string(), Box::new(room), Box::new(reconciler))
        .with_retry_config(RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        });
    let app = http::router(http::AppState {
        foreman: Arc::new(foreman),
        required_token: None,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn wait_until_settled(conn: &Conn, id: u64) {
    let mut cursor = 0u64;
    for _ in 0..500 {
        let progress = conn.get_progress(id, cursor).await.unwrap();
        cursor = progress.cursor;
        if progress.phase.is_terminal() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("reconcile {id} never settled");
}

#[tokio::test]
async fn a_host_already_holding_every_glyph_plans_as_a_provable_no_op() {
    let nginx = Glyph::AptPackage {
        name: "nginx".into(),
    };
    let motd = file_glyph("/etc/motd", "hello\n");
    let glyphs = vec![nginx.clone(), motd.clone()];
    let fake = glyphs.iter().fold(FakeReconciler::new(), |fake, glyph| {
        fake.preexisting(&glyph.key(), scroll_format::content_id_of_glyph(glyph))
    });
    let addr = serve_sharing_journal("h1", Arc::new(MemoryPlanRoom::new()), fake).await;
    let target = http_target("h1", addr);
    let conn = Conn::open(&target, &AuthSource::None).await.unwrap();
    let bytes = manifest_of("h1", glyphs);

    let body = conn.post_plan(bytes, true).await.unwrap();
    let plan: PlanResponse = serde_json::from_str(&body).unwrap();

    assert_eq!(plan.summary.install, 2, "{plan:?}");
    assert!(
        plan.ops.iter().all(|op| op.action == Action::Install),
        "journal must plan to install everything: {:?}",
        plan.ops
    );
    let reality = plan
        .reality
        .expect("--against-host carries a reality block");
    assert_eq!(reality.realized, 2, "{reality:?}");
    assert_eq!(reality.divergent, 0, "{reality:?}");
    assert!(reality.host_already_matches, "{reality:?}");

    let options = RenderOptions {
        against_host: true,
        ..Default::default()
    };
    let rendered = plan::present(&body, false, &options).unwrap();
    assert!(
        rendered.lines().any(|line| line == "  journal"),
        "{rendered}"
    );
    assert!(rendered.contains("+ install"), "{rendered}");
    assert!(rendered.lines().any(|line| line == "  host"), "{rendered}");
    assert!(rendered.contains("2 glyphs"), "{rendered}");
    assert!(
        rendered
            .lines()
            .any(|line| line == "  2 changes · 2 install"),
        "{rendered}"
    );
    assert!(
        rendered.lines().any(|line| line
            == "  host · every declared glyph already matches — applying this manifest changes nothing"),
        "{rendered}"
    );
}

#[tokio::test]
async fn a_glyph_changed_behind_golems_back_makes_the_two_columns_disagree() {
    let desired = file_glyph("/etc/motd", "hello\n");
    let drifted = file_glyph("/etc/motd", "ansible wrote this\n");
    let key = desired.key();
    assert_eq!(key, drifted.key(), "the drift must share the glyph's key");
    let drifted_cid = scroll_format::content_id_of_glyph(&drifted);

    let room = Arc::new(MemoryPlanRoom::new());
    let bytes = manifest_of("h1", vec![desired.clone()]);

    let apply_addr = serve_sharing_journal("h1", room.clone(), FakeReconciler::new()).await;
    let apply_conn = Conn::open(&http_target("h1", apply_addr), &AuthSource::None)
        .await
        .unwrap();
    let accepted = apply_conn.post_manifest(bytes.clone()).await.unwrap();
    wait_until_settled(&apply_conn, accepted.reconcile_id).await;

    let probe_fake = FakeReconciler::new().preexisting(&key, drifted_cid);
    let probe_addr = serve_sharing_journal("h1", room.clone(), probe_fake).await;
    let probe_conn = Conn::open(&http_target("h1", probe_addr), &AuthSource::None)
        .await
        .unwrap();

    let body = probe_conn.post_plan(bytes, true).await.unwrap();
    let plan: PlanResponse = serde_json::from_str(&body).unwrap();

    let op = plan
        .ops
        .iter()
        .find(|op| op.glyph_key == key)
        .unwrap_or_else(|| panic!("no op for {key}: {:?}", plan.ops));
    assert_eq!(
        op.action,
        Action::Noop,
        "the journal must be current: {op:?}"
    );
    assert_eq!(
        op.old_cid, op.new_cid,
        "a noop's content id must be unchanged: {op:?}"
    );
    assert_eq!(
        op.observed,
        Some(Observed::Divergent),
        "the host must disagree with what the journal believes: {op:?}"
    );
    let reality = plan
        .reality
        .expect("--against-host carries a reality block");
    assert_eq!(reality.divergent, 1, "{reality:?}");
    assert!(!reality.host_already_matches, "{reality:?}");

    let options = RenderOptions {
        against_host: true,
        ..Default::default()
    };
    let rendered = plan::present(&body, false, &options).unwrap();
    assert!(
        !rendered.lines().any(|line| line == "  journal"),
        "a noop-only plan carries no visible journal step: {rendered}"
    );
    assert!(rendered.lines().any(|line| line == "  host"), "{rendered}");
    assert!(rendered.contains("differs"), "{rendered}");
    assert!(rendered.contains("/etc/motd"), "{rendered}");
    assert!(
        rendered
            .lines()
            .any(|line| line == "  no changes · 1 unchanged"),
        "{rendered}"
    );
    assert!(
        rendered.lines().any(|line| line == "  host · 1 disagree"),
        "{rendered}"
    );
}

#[tokio::test]
async fn a_host_plan_writes_no_revision() {
    let nginx = Glyph::AptPackage {
        name: "nginx".into(),
    };
    let cid = scroll_format::content_id_of_glyph(&nginx);
    let fake = FakeReconciler::new().preexisting(&nginx.key(), cid);
    let addr = serve_sharing_journal("h1", Arc::new(MemoryPlanRoom::new()), fake).await;
    let target = http_target("h1", addr);
    let conn = Conn::open(&target, &AuthSource::None).await.unwrap();
    let bytes = manifest_of("h1", vec![nginx]);

    conn.post_plan(bytes, true).await.unwrap();

    let revisions = conn.get_json("revisions").await.unwrap();
    let journalled = revisions.as_array().unwrap();
    assert_eq!(journalled.len(), 1, "a host plan journalled: {revisions}");
    assert_eq!(journalled[0]["kind"], "init");
    let state = conn.get_json("state").await.unwrap();
    assert!(
        state["content_id"].is_null(),
        "a host plan applied something: {state}"
    );
}

#[tokio::test]
async fn a_single_unknown_glyph_denies_host_already_matches() {
    // A scroll with *only* the unknown glyph would deny `host_already_matches`
    // even if `Reality::over` dropped its `unknown == 0` clause entirely —
    // `realized + already_gone == 0` denies it on its own. Seed a glyph the
    // host genuinely already holds alongside the sealed one, so this test can
    // only pass if the unknown glyph is the thing denying the match.
    let nginx = Glyph::AptPackage {
        name: "nginx".into(),
    };
    let sealed = sealed_file_glyph("/etc/app/creds.conf");
    let fake = FakeReconciler::new()
        .with_keyring(Keyring::without_key())
        .preexisting(&nginx.key(), scroll_format::content_id_of_glyph(&nginx));
    let addr = serve_sharing_journal("h1", Arc::new(MemoryPlanRoom::new()), fake).await;
    let target = http_target("h1", addr);
    let conn = Conn::open(&target, &AuthSource::None).await.unwrap();
    let bytes = manifest_of("h1", vec![nginx, sealed]);

    let body = conn.post_plan(bytes, true).await.unwrap();
    let plan: PlanResponse = serde_json::from_str(&body).unwrap();
    let reality = plan
        .reality
        .expect("--against-host carries a reality block");

    assert_eq!(reality.realized, 1, "{reality:?}");
    assert_eq!(reality.unknown, 1, "{reality:?}");
    assert!(!reality.host_already_matches, "{reality:?}");
}
