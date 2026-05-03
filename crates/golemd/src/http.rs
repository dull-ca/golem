//! Tiny HTTP server for operator pushes and status checks.
//!
//! Endpoints:
//!   POST /bundle   — accept a SignedBundle JSON. Verify, swap into shared
//!                    state, and return 202 Accepted. Reconciler will pick
//!                    it up on the next tick.
//!   GET  /status   — current bundle version + last tick summary.
//!   GET  /healthz  — process liveness.
//!
//! Bind to localhost or a Unix socket; trust comes from the ed25519
//! signature on the bundle, not from network ACLs. (You'd still want to
//! restrict the socket — over Nebula, or unix-domain only — but the
//! cryptographic story is what makes this safe.)

use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use golem_types::Bundle;
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::bundle::{load_signed, TrustConfig};

#[derive(Clone)]
pub struct AppState {
    pub trust:  Arc<TrustConfig>,
    pub bundle: Arc<RwLock<Option<Bundle>>>,
}

#[derive(Serialize)]
struct StatusResp {
    bundle_version: Option<u64>,
    node:           String,
    claim_count:    usize,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/bundle",  post(post_bundle))
        .route("/status",  get(get_status))
        .route("/healthz", get(get_healthz))
        .with_state(state)
}

async fn get_healthz() -> &'static str { "ok\n" }

async fn get_status(State(s): State<AppState>) -> Json<StatusResp> {
    let b = s.bundle.read().await;
    Json(StatusResp {
        bundle_version: b.as_ref().map(|b| b.version),
        node:           s.trust.node_name.clone(),
        claim_count:    b.as_ref().map(|b| b.claims.len()).unwrap_or(0),
    })
}

async fn post_bundle(
    State(s): State<AppState>,
    body: axum::body::Bytes,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    // TOCTOU fix: hold the write-guard across the entire read-validate-write
    // window. Without this, two concurrent POSTs both reading prev_version=N
    // would both accept their (N+1, N+2) bundles in the wrong order — older
    // wins. With the guard held, the second writer reads the first writer's
    // new prev_version and rejects via the monotonicity check inside
    // load_signed.
    let mut slot = s.bundle.write().await;
    let prev_version = slot.as_ref().map(|b| b.version);

    let new_bundle = match load_signed(&body, &s.trust, prev_version) {
        Ok(b) => b,
        Err(e) => {
            warn!("rejected bundle: {e:#}");
            return Err((StatusCode::BAD_REQUEST, format!("{e:#}")));
        }
    };

    info!(
        "accepted bundle version={} claims={}",
        new_bundle.version,
        new_bundle.claims.len()
    );
    let version = new_bundle.version;
    *slot = Some(new_bundle);

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "accepted_version": version })),
    ))
}
