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
async fn no_header_is_401_when_token_configured() {
    let base = serve(Some("secret")).await;

    let resp = reqwest::get(format!("{base}/status")).await.unwrap();

    assert_eq!(resp.status().as_u16(), 401);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["kind"], "unauthorized");
}

#[tokio::test]
async fn wrong_token_is_401() {
    let base = serve(Some("secret")).await;

    let resp = reqwest::Client::new()
        .get(format!("{base}/status"))
        .header("Authorization", "Bearer nope")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status().as_u16(), 401);
}

#[tokio::test]
async fn correct_token_reaches_status_and_manifest() {
    let base = serve(Some("secret")).await;

    let status_resp = reqwest::Client::new()
        .get(format!("{base}/status"))
        .header("Authorization", "Bearer secret")
        .send()
        .await
        .unwrap();
    assert_eq!(status_resp.status().as_u16(), 200);

    let manifest_resp = reqwest::Client::new()
        .post(format!("{base}/manifest"))
        .header("Authorization", "Bearer secret")
        .header("content-type", "application/octet-stream")
        .body(manifest_bytes())
        .send()
        .await
        .unwrap();
    assert_eq!(manifest_resp.status().as_u16(), 202);
}

#[tokio::test]
async fn no_token_configured_leaves_routes_open() {
    let base = serve(None).await;

    let resp = reqwest::get(format!("{base}/status")).await.unwrap();

    assert_eq!(resp.status().as_u16(), 200);
}
