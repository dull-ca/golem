//! The HTTP adapter over the foreman (ADR 0014 §5). `POST /manifest` ingests
//! raw manifest bytes and reconciles; the read routes expose the current applied
//! scroll (`GET /state`), the journal (`GET /revisions[/:id]`), and a liveness
//! summary (`GET /status`). There is no decommission verb — a node's state is a
//! whole scroll, so "remove everything" is applying an empty one. Foreman calls
//! are blocking, so each runs on `spawn_blocking`.

use axum::{
    body::Bytes,
    extract::{Path, State as AxState},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Serialize;
use std::sync::Arc;

use crate::foreman::Foreman;
use crate::journal::AppliedState;

#[derive(Clone)]
pub struct AppState {
    pub foreman: Arc<Foreman>,
}

/// Wire the five routes to the shared foreman state.
pub fn router(app: AppState) -> Router {
    Router::new()
        .route("/manifest", post(apply_manifest))
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

async fn apply_manifest(
    AxState(s): AxState<AppState>,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let bytes = body.to_vec();
    let foreman = s.foreman.clone();
    let report = tokio::task::spawn_blocking(move || foreman.apply_manifest(&bytes))
        .await
        .map_err(|e| ApiError::internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(ApiError::from_foreman)?;
    Ok(Json(report))
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
}

impl ApiError {
    fn from_foreman(e: crate::foreman::ForemanError) -> Self {
        let status = match e {
            crate::foreman::ForemanError::WalUnreadable { .. }
            | crate::foreman::ForemanError::ManifestUndecodable { .. }
            | crate::foreman::ForemanError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        ApiError {
            status,
            kind: e.kind().to_string(),
            message: e.message(),
        }
    }
    fn internal(e: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            kind: "internal".to_string(),
            message: format!("{e:#}"),
        }
    }
    fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            kind: "not-found".to_string(),
            message,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self)).into_response()
    }
}
