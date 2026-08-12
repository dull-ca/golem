use std::sync::Arc;

use golemd::fake_reconciler::FakeReconciler;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::planroom::MemoryPlanRoom;
use scroll_format::{Contents, Glyph, Manifest, Scroll};

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
