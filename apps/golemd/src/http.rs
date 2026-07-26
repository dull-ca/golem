//! The HTTP adapter over the foreman (ADR 0014 §5). `POST /manifest` ingests
//! raw manifest bytes and starts a reconcile, returning `202 { reconcile_id }`;
//! the client then polls `GET /reconciles/<id>|latest` until the projection
//! settles (the two-request protocol, ADR 0033 §1–2). The remaining read routes
//! expose the current applied scroll (`GET /state`), the journal
//! (`GET /revisions[/:id]`), and a liveness summary (`GET /status`). There is no
//! decommission verb — a node's state is a whole scroll, so "remove everything"
//! is applying an empty one. Foreman calls are blocking, so each runs on
//! `spawn_blocking`.

use axum::{
    body::Bytes,
    extract::{Path, Query, State as AxState},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::foreman::Foreman;
use crate::journal::AppliedState;

#[derive(Clone)]
pub struct AppState {
    pub foreman: Arc<Foreman>,
}

/// Wire the routes to the shared foreman state. `/reconciles/latest` is
/// registered before `/reconciles/:id` so `latest` is not captured as an id.
pub fn router(app: AppState) -> Router {
    Router::new()
        .route("/manifest", post(apply_manifest))
        .route("/reconciles/latest", get(reconcile_latest))
        .route("/reconciles/:id", get(reconcile))
        .route("/state", get(state))
        .route("/revisions", get(revisions))
        .route("/revisions/:id", get(revision))
        .route("/status", get(status))
        .with_state(app)
}

/// Run a blocking foreman call off the async runtime and map its errors to an
/// HTTP 500.
async fn blocking<T, F>(foreman: Arc<Foreman>, f: F) -> Result<T, ApiError>
where
    F: FnOnce(&Foreman) -> anyhow::Result<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || f(&foreman))
        .await
        .map_err(|e| ApiError::internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(ApiError::internal)
}

#[derive(Serialize)]
struct Status {
    host: String,
    latest_revision: Option<u64>,
}

async fn status(AxState(s): AxState<AppState>) -> Result<impl IntoResponse, ApiError> {
    let host = s.foreman.host().to_string();
    let latest = blocking(s.foreman.clone(), |f| f.latest_revision_id()).await?;
    Ok(Json(Status {
        host,
        latest_revision: latest,
    }))
}

#[derive(Serialize)]
struct Accepted {
    reconcile_id: u64,
}

/// Ingest synchronously, then run the reconcile detached and return `202` at
/// once (ADR 0033 §1). Ingest does the cheap work — decode, host-scroll select,
/// the in-progress gate — so its failures come back on *this* request as typed
/// non-2xx (`ApiError::from_foreman`); the reconcile that follows reports its
/// own per-glyph outcomes only through the poll path. Only a reconcile that
/// actually started yields a `reconcile_id` to poll.
async fn apply_manifest(
    AxState(s): AxState<AppState>,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let bytes = body.to_vec();
    let foreman = s.foreman.clone();
    let (reconcile_id, selected) = tokio::task::spawn_blocking(move || foreman.ingest(&bytes))
        .await
        .map_err(|e| ApiError::internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(ApiError::from_foreman)?;
    let foreman = s.foreman.clone();
    tokio::task::spawn_blocking(move || {
        if let Err(e) = foreman.run_reconcile(reconcile_id, selected) {
            tracing::error!(reconcile_id, error = %e, "reconcile run failed");
        }
    });
    Ok((StatusCode::ACCEPTED, Json(Accepted { reconcile_id })))
}

#[derive(Deserialize)]
struct After {
    after: Option<u64>,
}

async fn reconcile(
    AxState(s): AxState<AppState>,
    Path(id): Path<u64>,
    Query(q): Query<After>,
) -> Result<impl IntoResponse, ApiError> {
    let after = q.after.unwrap_or(0);
    match blocking(s.foreman.clone(), move |f| f.progress_projection(id, after)).await? {
        Some(p) => Ok(Json(p)),
        None => Err(ApiError::not_found(format!("no reconcile {id}"))),
    }
}

async fn reconcile_latest(
    AxState(s): AxState<AppState>,
    Query(q): Query<After>,
) -> Result<impl IntoResponse, ApiError> {
    let after = q.after.unwrap_or(0);
    let latest = blocking(s.foreman.clone(), |f| f.latest_reconcile_id()).await?;
    let Some(id) = latest else {
        return Err(ApiError::not_found("no reconcile attempts yet".into()));
    };
    match blocking(s.foreman.clone(), move |f| f.progress_projection(id, after)).await? {
        Some(p) => Ok(Json(p)),
        None => Err(ApiError::not_found(format!("no reconcile {id}"))),
    }
}

#[derive(Serialize)]
struct StateView {
    content_id: Option<String>,
    scroll: Option<scroll_format::Scroll>,
}

async fn state(AxState(s): AxState<AppState>) -> Result<impl IntoResponse, ApiError> {
    let applied: Option<AppliedState> = blocking(s.foreman.clone(), |f| f.applied_state()).await?;
    let view = match applied {
        Some(a) => StateView {
            content_id: Some(a.scroll_content_id.to_string()),
            scroll: Some(a.scroll),
        },
        None => StateView {
            content_id: None,
            scroll: None,
        },
    };
    Ok(Json(view))
}

async fn revisions(AxState(s): AxState<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(s.foreman.clone(), |f| f.revisions()).await?))
}

async fn revision(
    AxState(s): AxState<AppState>,
    Path(id): Path<u64>,
) -> Result<impl IntoResponse, ApiError> {
    match blocking(s.foreman.clone(), move |f| f.revision(id)).await? {
        Some(rev) => Ok(Json(rev)),
        None => Err(ApiError::not_found(format!("no revision {id}"))),
    }
}

/// A structured error body (`{ kind, message }`) with a status held out of the
/// JSON (ADR 0029 §5). Reserved for genuine daemon/transport failures — a
/// reconcile that ran and reported per-glyph failures is a 200 with a
/// `ReconcileReport`, not an `ApiError`.
#[derive(Serialize)]
struct ApiError {
    #[serde(skip)]
    status: StatusCode,
    kind: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reconcile_id: Option<u64>,
}

impl ApiError {
    /// Maps ingest failures — the reasons a reconcile never started — to their
    /// synchronous status (ADR 0033 §1). `ReconcileInProgress` is a `409` that
    /// carries the id of the attempt already running so the caller can poll
    /// *it* instead of retrying; the rest are `500`-class daemon faults.
    fn from_foreman(e: crate::foreman::ForemanError) -> Self {
        use crate::foreman::ForemanError::*;
        let reconcile_id = match &e {
            ReconcileInProgress { reconcile_id } => Some(*reconcile_id),
            _ => None,
        };
        let status = match e {
            WalUnreadable { .. } | ManifestUndecodable { .. } | Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            ReconcileInProgress { .. } => StatusCode::CONFLICT,
        };
        ApiError {
            status,
            kind: e.kind().to_string(),
            message: e.message(),
            reconcile_id,
        }
    }
    fn internal(e: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            kind: "internal".to_string(),
            message: format!("{e:#}"),
            reconcile_id: None,
        }
    }
    fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            kind: "not-found".to_string(),
            message,
            reconcile_id: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self)).into_response()
    }
}
