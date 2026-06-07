//! HTTP surface over the foreman.
//!
//! - `POST   /blueprints`        commission
//! - `DELETE /blueprints/:name`  decommission
//! - `GET    /blueprints`        list blueprints
//! - `GET    /state`             resolved state
//! - `GET    /revisions[/:id]`   the journal
//! - `GET    /status`            host + latest revision
//!
//! The foreman is synchronous and blocking (sqlite, sleeps, retries), so every
//! call runs on a blocking thread via [`blocking`], never on a runtime worker.

use axum::{
    extract::{Path, State as AxState},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use golem_types::Blueprint;
use serde::Serialize;
use std::sync::Arc;

use crate::foreman::Foreman;

#[derive(Clone)]
pub struct AppState {
    pub foreman: Arc<Foreman>,
}

pub fn router(app: AppState) -> Router {
    Router::new()
        .route("/blueprints", post(commission).get(blueprints))
        .route("/blueprints/:name", delete(decommission))
        .route("/state", get(state))
        .route("/revisions", get(revisions))
        .route("/revisions/:id", get(revision))
        .route("/status", get(status))
        .with_state(app)
}

/// Run blocking foreman work off the async runtime.
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
    Ok(Json(Status { host, latest_revision: latest }))
}

async fn commission(
    AxState(s): AxState<AppState>,
    Json(bp): Json<Blueprint>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(s.foreman.clone(), move |f| f.commission(bp)).await?))
}

async fn decommission(
    AxState(s): AxState<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let n = name.clone();
    match blocking(s.foreman.clone(), move |f| f.decommission(&n)).await? {
        Some(rev) => Ok(Json(rev)),
        None => Err(ApiError::not_found(format!("no blueprint named {name:?}"))),
    }
}

async fn blueprints(AxState(s): AxState<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(s.foreman.clone(), |f| f.blueprints()).await?))
}

async fn state(AxState(s): AxState<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(blocking(s.foreman.clone(), |f| f.state()).await?))
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

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn internal(e: anyhow::Error) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, message: format!("{e:#}") }
    }
    fn not_found(message: String) -> Self {
        Self { status: StatusCode::NOT_FOUND, message }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, self.message).into_response()
    }
}
