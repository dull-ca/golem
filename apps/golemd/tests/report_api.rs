use std::sync::Arc;

use golemd::config::{OnExhaustConfig, RetryConfig};
use golemd::foreman::Foreman;
use golemd::http;
use golemd::journal::{GlyphOp, Outcome};
use golemd::planroom::MemoryPlanRoom;
use golemd::reconciler::{inverse_of, EnactError, EnactResult, Reconciler};
use scroll_format::{ContentId, Contents, Glyph, Manifest, Scroll};

struct FailOne {
    bad: String,
}
impl Reconciler for FailOne {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        if glyph.key() == self.bad {
            return Err(EnactError::Fatal("scripted".into()));
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

fn apt(name: &str) -> Glyph {
    Glyph::AptPackage { name: name.into() }
}

fn manifest_bytes() -> Vec<u8> {
    let host = Scroll {
        name: "h1".into(),
        policy: None,
        contents: Contents::Glyphs(vec![apt("bad")]),
    };
    scroll_format::to_bytes(&Manifest::from_scrolls(vec![host], "test"))
}

async fn serve(foreman: Foreman) -> String {
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

#[tokio::test]
async fn a_failing_reconcile_settles_rolled_back_via_poll() {
    let foreman = Foreman::new(
        "h1".into(),
        Box::new(MemoryPlanRoom::new()),
        Box::new(FailOne {
            bad: "apt:bad".into(),
        }),
    )
    .with_retry_config(RetryConfig {
        max_attempts: 1,
        base_delay_ms: 0,
        on_exhaust: OnExhaustConfig::Rollback,
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

    for _ in 0..50 {
        let p: serde_json::Value = reqwest::get(format!("{base}/reconciles/{id}"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if !p["report"].is_null() {
            assert_eq!(p["report"]["outcome"], "rolled_back");
            assert_eq!(p["report"]["units"][0]["failures"][0]["class"], "fatal");
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("never settled");
}

#[tokio::test]
async fn an_undecodable_manifest_is_a_structured_500() {
    let foreman = Foreman::new(
        "h1".into(),
        Box::new(MemoryPlanRoom::new()),
        Box::new(FailOne { bad: "none".into() }),
    );
    let base = serve(foreman).await;

    let resp = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("content-type", "application/octet-stream")
        .body(b"not a manifest".to_vec())
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["kind"], "manifest-undecodable");
}
