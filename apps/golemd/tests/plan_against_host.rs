use std::sync::Arc;
use std::time::Duration;

use golemd::fake_reconciler::FakeReconciler;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::journal::{GlyphOp, Outcome};
use golemd::planroom::MemoryPlanRoom;
use golemd::reconciler::{inverse_of, EnactResult, Reconciler};
use scroll_format::{ContentId, Contents, Glyph, Manifest, Scroll};

fn apt(name: &str) -> Glyph {
    Glyph::AptPackage { name: name.into() }
}

fn manifest_bytes() -> Vec<u8> {
    let host = Scroll {
        name: "h1".into(),
        policy: None,
        notifies: vec![],
        contents: Contents::Glyphs(vec![apt("ok")]),
    };
    scroll_format::to_bytes(&Manifest::from_scrolls(vec![host], "test"))
}

/// A reconciler whose `apply` parks until released (the same shape as
/// `async_apply.rs`'s `GatedOk`) — lets a test genuinely hold `run_reconcile`'s
/// write lock open across an HTTP request, rather than synthesising the
/// contention.
struct GatedApply {
    entered: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
impl Reconciler for GatedApply {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let _ = self.entered.send(());
        // Ignoring a disconnected `release` (the sender dropped without
        // sending, e.g. an assertion panicked earlier in the test) is what
        // lets this thread unpark and unwind instead of hanging the whole
        // run during a failing test's panic.
        let _ = self.release.lock().unwrap().recv();
        Ok(Outcome {
            op: GlyphOp::Install {
                cid,
                glyph: glyph.clone(),
            },
            cid,
            inverse: inverse_of(glyph),
            changed: true,
        })
    }
    fn reverse(&self, _o: &Outcome) -> EnactResult<()> {
        Ok(())
    }
}

/// Serves a foreman whose reconciler parks on the first `apply`, so a caller
/// can `POST /manifest` to put a real reconcile in flight — write lock held —
/// then drive `/plan` requests against it before sending on `release`.
async fn serve_gated() -> (
    String,
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::Sender<()>,
) {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let foreman = Foreman::new(
        "h1".into(),
        Box::new(MemoryPlanRoom::new()),
        Box::new(GatedApply {
            entered: entered_tx,
            release: std::sync::Mutex::new(release_rx),
        }),
    );
    let app = http::router(http::AppState {
        foreman: Arc::new(foreman),
        required_token: None,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), entered_rx, release_tx)
}

async fn serve(required_token: Option<&str>) -> String {
    let foreman = Foreman::new(
        "h1".into(),
        Box::new(MemoryPlanRoom::new()),
        Box::new(FakeReconciler::new()),
    );
    let app = http::router(http::AppState {
        foreman: Arc::new(foreman),
        required_token: required_token.map(|t| Arc::new(t.to_string())),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn a_plan_with_no_query_string_is_journal_only() {
    let base = serve(None).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/plan"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("reality").is_none());
    assert!(body["ops"][0].get("observed").is_none());
}

#[tokio::test]
async fn a_plan_with_against_host_true_reads_the_host() {
    let base = serve(None).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/plan?against_host=true"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("reality").is_some());
    assert!(body["ops"][0].get("observed").is_some());
}

#[tokio::test]
async fn a_plan_with_against_host_false_is_journal_only() {
    let base = serve(None).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/plan?against_host=false"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body.get("reality").is_none());
}

/// Pins the actual behaviour of a malformed `against_host` value now that
/// `?against_host=` goes through an axum `Query<PlanQuery>` extractor rather
/// than being ignored outright. No known client sends anything but `true`,
/// `false`, or an absent parameter, so this only records what happens — it
/// does not assert what *should* happen.
#[tokio::test]
async fn a_malformed_against_host_value_is_recorded_not_assumed() {
    let base = serve(None).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/plan?against_host=banana"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 400);
}

#[tokio::test]
async fn the_auth_gate_still_covers_the_host_plan() {
    let base = serve(Some("secret")).await;

    let no_token = reqwest::Client::new()
        .post(format!("{base}/plan?against_host=true"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(no_token.status().as_u16(), 401);

    let with_token = reqwest::Client::new()
        .post(format!("{base}/plan?against_host=true"))
        .header("Authorization", "Bearer secret")
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(with_token.status().as_u16(), 200);
}

#[tokio::test]
async fn a_journal_only_plan_over_http_still_returns_200_during_an_apply() {
    let (base, entered_rx, release_tx) = serve_gated().await;

    let apply_resp = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(apply_resp.status().as_u16(), 202);

    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the gated apply to enter and park, write lock held");

    // golemctl's own client sets no timeout, so a regression that made
    // `JournalOnly` block on the write lock would otherwise hang this test
    // (and the whole suite) rather than fail it. Bound the wait here instead,
    // so that regression is a clear timeout, not a CI hang diagnosed as
    // flaky infra.
    let plan_resp = tokio::time::timeout(
        Duration::from_secs(5),
        reqwest::Client::new()
            .post(format!("{base}/plan"))
            .header("content-type", "application/octet-stream")
            .body(manifest_bytes())
            .send(),
    )
    .await
    .expect("a journal-only plan must not block on a live apply's write lock")
    .unwrap();
    assert_eq!(
        plan_resp.status().as_u16(),
        200,
        "an ordinary golemctl plan must never be gated by a live apply"
    );

    release_tx.send(()).unwrap();
}

#[tokio::test]
async fn a_host_plan_over_http_409s_while_an_apply_holds_the_write_lock() {
    let (base, entered_rx, release_tx) = serve_gated().await;

    let apply_resp = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(apply_resp.status().as_u16(), 202);

    entered_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the gated apply to enter and park, write lock held");

    // This is precisely the test whose purpose is that a host plan never
    // queues behind an apply: `write_activity_snapshot` must answer
    // `HostBusy` immediately, never block. golemctl's own client sets no
    // timeout either, so a regression that made this request block on the
    // write lock would otherwise hang this test (and the whole suite)
    // rather than fail it. Bound the wait here instead, the same way the
    // journal-only test above does.
    let plan_resp = tokio::time::timeout(
        Duration::from_secs(5),
        reqwest::Client::new()
            .post(format!("{base}/plan?against_host=true"))
            .header("content-type", "application/octet-stream")
            .body(manifest_bytes())
            .send(),
    )
    .await
    .expect("a host-probing plan must not block on a live apply's write lock")
    .unwrap();
    assert_eq!(plan_resp.status().as_u16(), 409);
    let body: serde_json::Value = plan_resp.json().await.unwrap();
    assert_eq!(body["kind"], "host-busy");

    release_tx.send(()).unwrap();
}
