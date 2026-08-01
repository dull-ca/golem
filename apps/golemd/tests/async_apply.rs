use std::sync::Arc;
use std::time::Duration;

use golemd::config::RetryConfig;
use golemd::foreman::Foreman;
use golemd::host::CommandSink;
use golemd::http;
use golemd::journal::{GlyphOp, Outcome};
use golemd::planroom::MemoryPlanRoom;
use golemd::progress::{EventKind, EventLevel};
use golemd::reconciler::{inverse_of, EnactResult, Reconciler};
use scroll_format::{ContentId, Contents, Glyph, Manifest, Scroll};

fn quiet_retry() -> RetryConfig {
    RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        ..Default::default()
    }
}

struct Ok1;
impl Reconciler for Ok1 {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
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

/// Applies every glyph like `Ok1`, except one key whose `apply` panics — the
/// spawned reconcile's failure mode the panic guard must contain (ADR 0033 §1).
struct PanicOn {
    key: String,
}
impl Reconciler for PanicOn {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        if glyph.key() == self.key {
            panic!("simulated panic applying {}", self.key);
        }
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

/// Opts into streaming (ADR 0033 §2): `apply_streaming` forwards two scripted
/// stdout lines to the sink before settling like `Ok1`, so the whole path —
/// reconciler sink → foreman ring `record_kind(Cmd)` → projection `events` —
/// can be asserted end to end. Its plain `apply` (the default the port would
/// otherwise reach) emits nothing, matching the production contract.
struct StreamingOk {
    lines: Vec<String>,
}
impl Reconciler for StreamingOk {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
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
    fn apply_streaming(
        &self,
        glyph: &Glyph,
        cid: ContentId,
        sink: &mut CommandSink<'_>,
    ) -> EnactResult<Outcome> {
        for line in &self.lines {
            sink(EventLevel::Info, line);
        }
        self.apply(glyph, cid)
    }
    fn reverse(&self, _o: &Outcome) -> EnactResult<()> {
        Ok(())
    }
}

struct GatedOk {
    entered_apply: std::sync::mpsc::Sender<()>,
    release_apply: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}
impl Reconciler for GatedOk {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let _ = self.entered_apply.send(());
        let _ = self.release_apply.lock().unwrap().recv();
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

fn manifest_bytes() -> Vec<u8> {
    let host = Scroll {
        name: "h1".into(),
        policy: None,
        notifies: vec![],
        contents: Contents::Glyphs(vec![Glyph::AptPackage {
            name: "nginx".into(),
        }]),
    };
    scroll_format::to_bytes(&Manifest::from_scrolls(vec![host], "test"))
}

async fn serve(foreman: Foreman) -> String {
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

#[tokio::test]
async fn apply_returns_202_then_polls_to_settled_with_report() {
    let foreman = Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(Ok1))
        .with_retry_config(RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        });
    let base = serve(foreman).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status().as_u16(), 202);
    let id = resp.json::<serde_json::Value>().await.unwrap()["reconcile_id"]
        .as_u64()
        .unwrap();

    let mut cursor = 0u64;
    let mut settled = None;
    for _ in 0..50 {
        let p: serde_json::Value = reqwest::get(format!("{base}/reconciles/{id}?after={cursor}"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        cursor = p["cursor"].as_u64().unwrap();
        if p["phase"] == "settled" {
            settled = Some(p);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let p = settled.expect("reconcile settled");
    assert_eq!(p["report"]["outcome"], "settled");
    assert_eq!(p["units"][0]["glyphs"][0]["glyph_key"], "apt:nginx");
    assert_eq!(p["units"][0]["glyphs"][0]["state"], "applied");
}

#[tokio::test]
async fn a_concurrent_post_while_run_reconcile_holds_the_write_lock_blocks_rather_than_409s() {
    let (entered_apply_tx, entered_apply_rx) = std::sync::mpsc::channel();
    let (release_apply_tx, release_apply_rx) = std::sync::mpsc::channel();
    let foreman = Foreman::new(
        "h1".into(),
        Box::new(MemoryPlanRoom::new()),
        Box::new(GatedOk {
            entered_apply: entered_apply_tx,
            release_apply: std::sync::Mutex::new(release_apply_rx),
        }),
    )
    .with_retry_config(RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        ..Default::default()
    });
    let base = serve(foreman).await;

    let first = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(first.status().as_u16(), 202);

    entered_apply_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("the first reconcile's apply() to start");

    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        release_apply_tx.send(()).unwrap();
    });

    let second = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();

    assert_eq!(second.status().as_u16(), 202);
}

#[tokio::test]
async fn a_manifest_posted_against_an_unsettled_attempt_gets_a_real_http_409() {
    let foreman = Arc::new(
        Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(Ok1))
            .with_retry_config(RetryConfig {
                max_attempts: 1,
                base_delay_ms: 0,
                ..Default::default()
            }),
    );

    let ingest_foreman = foreman.clone();
    let (first_id, _selected) = tokio::task::spawn_blocking(move || {
        ingest_foreman
            .ingest(&manifest_bytes())
            .expect("ingest opens the unsettled attempt")
    })
    .await
    .unwrap();

    let app = http::router(http::AppState {
        foreman: foreman.clone(),
        required_token: None,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let base = format!("http://{addr}");

    let second = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(second.status().as_u16(), 409);
    let body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(body["kind"], "reconcile-in-progress");
    assert_eq!(body["reconcile_id"], first_id);
}

#[test]
fn cmd_output_flows_from_the_reconciler_sink_to_the_projection_tagged_cmd() {
    let reconciler = StreamingOk {
        lines: vec![
            "Unpacking nginx (1.24.0) ...".to_string(),
            "Setting up nginx ...".to_string(),
        ],
    };
    let f = Foreman::new(
        "h1".into(),
        Box::new(MemoryPlanRoom::new()),
        Box::new(reconciler),
    )
    .with_retry_config(quiet_retry());
    let (id, selected) = f.ingest(&manifest_bytes()).unwrap();
    f.run_reconcile(id, selected).unwrap();

    let p = f.progress_projection(id, 0).unwrap().unwrap();
    let cmd: Vec<&golemd::progress::ProgressEvent> = p
        .events
        .iter()
        .filter(|e| e.kind == EventKind::Cmd)
        .collect();
    assert_eq!(
        cmd.len(),
        2,
        "both scripted command lines reach the projection, saw {:?}",
        p.events
    );
    assert_eq!(cmd[0].message, "Unpacking nginx (1.24.0) ...");
    assert_eq!(cmd[0].glyph_key, "apt:nginx");
    assert!(p.events.iter().any(|e| e.kind == EventKind::Lifecycle));
    let cmd_seqs: Vec<u64> = cmd.iter().map(|e| e.seq).collect();
    assert!(
        cmd_seqs.windows(2).all(|w| w[0] < w[1]),
        "cmd events keep the shared monotone seq"
    );
}

#[tokio::test]
async fn latest_returns_the_most_recent_attempt() {
    let foreman = Foreman::new("h1".into(), Box::new(MemoryPlanRoom::new()), Box::new(Ok1))
        .with_retry_config(RetryConfig {
            max_attempts: 1,
            base_delay_ms: 0,
            ..Default::default()
        });
    let base = serve(foreman).await;
    reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    for _ in 0..50 {
        let p: serde_json::Value = reqwest::get(format!("{base}/reconciles/latest"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if p["phase"] == "settled" {
            assert_eq!(p["reconcile_id"], 1);
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("latest never settled");
}

#[test]
fn a_settled_attempt_reports_after_the_in_memory_cache_is_lost() {
    let room = Arc::new(MemoryPlanRoom::new());

    let f1 = Foreman::new("h1".into(), Box::new(room.clone()), Box::new(Ok1))
        .with_retry_config(quiet_retry());
    let (id, selected) = f1.ingest(&manifest_bytes()).unwrap();
    let live = f1.progress_projection(id, 0).unwrap().unwrap();
    assert_eq!(live.phase, golemd::projection::PhaseView::Enacting);
    f1.run_reconcile(id, selected).unwrap();
    drop(f1);

    let f2 = Foreman::new("h1".into(), Box::new(room.clone()), Box::new(Ok1))
        .with_retry_config(quiet_retry());
    let p = f2
        .progress_projection(id, 0)
        .unwrap()
        .expect("the settled attempt is still projectable after restart");

    assert_eq!(p.phase, golemd::projection::PhaseView::Settled);
    let report = p
        .report
        .expect("a settled attempt always yields a report, rebuilt from the WAL on cache miss");
    assert_eq!(report.outcome, golemd::report::TopOutcome::Settled);
    assert_eq!(report.units.len(), 1);
    assert_eq!(report.units[0].unit_path, vec!["h1".to_string()]);
    assert_eq!(report.units[0].glyphs[0].glyph_key, "apt:nginx");
}

#[test]
fn a_panic_in_the_reconcile_is_contained_and_the_daemon_keeps_serving() {
    use golemd::planroom::PlanRoom;

    let room = Arc::new(MemoryPlanRoom::new());
    let reconciler = golemd::reconciler::PanicCatching::new(PanicOn {
        key: "apt:nginx".into(),
    });
    let f = Foreman::new("h1".into(), Box::new(room.clone()), Box::new(reconciler))
        .with_retry_config(quiet_retry());

    let (id, selected) = f.ingest(&manifest_bytes()).unwrap();
    f.run_reconcile_guarded(id, selected);

    let attempt = room.latest_attempt().unwrap().unwrap();
    assert!(
        attempt.phase.is_settled(),
        "a panicking reconcile ends the attempt terminally, not wedged unsettled: {:?}",
        attempt.phase
    );

    let p = f
        .progress_projection(id, 0)
        .unwrap()
        .expect("the panicked attempt is still projectable");
    assert!(
        matches!(
            p.phase,
            golemd::projection::PhaseView::Settled | golemd::projection::PhaseView::RolledBack
        ),
        "the poll shows a terminal phase: {:?}",
        p.phase
    );
    let report = p.report.expect("a settled attempt always yields a report");
    assert!(
        matches!(
            report.outcome,
            golemd::report::TopOutcome::RolledBack | golemd::report::TopOutcome::Partial
        ),
        "the panicked glyph is reported as a fatal failure, its unit undone: {:?}",
        report.outcome
    );

    let (next_id, _selected) = f
        .ingest(&manifest_bytes())
        .expect("a subsequent apply is accepted, not permanently 409'd by a poisoned lock");
    assert_ne!(next_id, id, "the next apply opens a fresh attempt");
}
