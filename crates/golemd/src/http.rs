//! HTTP routes.
//!
//! - `POST   /blueprints`              commission (insert or replace)
//! - `DELETE /blueprints/:name`        decommission
//! - `GET    /blueprints`              list active
//! - `GET    /state`                   current resolved state
//! - `GET    /revisions`               full journal
//! - `GET    /revisions/:id`           a single revision (state + actions)
//! - `GET    /status`                  identity + latest revision id

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

use crate::store::Store;

#[derive(Clone)]
pub struct AppState {
    pub node: String,
    pub store: Arc<Store>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/blueprints", post(commission).get(list_blueprints))
        .route("/blueprints/:name", delete(decommission))
        .route("/state", get(current_state))
        .route("/revisions", get(list_revisions))
        .route("/revisions/:id", get(get_revision))
        .route("/status", get(status))
        .with_state(state)
}

#[derive(Serialize)]
struct StatusBody {
    node: String,
    latest_revision: Option<u64>,
}

async fn status(AxState(s): AxState<AppState>) -> impl IntoResponse {
    let latest = s
        .store
        .list_revisions()
        .map(|rs| rs.last().map(|r| r.id))
        .unwrap_or(None);
    Json(StatusBody {
        node: s.node,
        latest_revision: latest,
    })
}

async fn commission(
    AxState(s): AxState<AppState>,
    Json(bp): Json<Blueprint>,
) -> Result<impl IntoResponse, ApiError> {
    let rev = s.store.commission(bp).map_err(ApiError::internal)?;
    Ok(Json(rev))
}

async fn decommission(
    AxState(s): AxState<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    match s.store.decommission(&name).map_err(ApiError::internal)? {
        Some(rev) => Ok(Json(rev)),
        None => Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: format!("no blueprint named {name:?}"),
        }),
    }
}

async fn list_blueprints(AxState(s): AxState<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(s.store.list_blueprints().map_err(ApiError::internal)?))
}

async fn current_state(AxState(s): AxState<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(s.store.current_state().map_err(ApiError::internal)?))
}

async fn list_revisions(AxState(s): AxState<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(s.store.list_revisions().map_err(ApiError::internal)?))
}

async fn get_revision(
    AxState(s): AxState<AppState>,
    Path(id): Path<u64>,
) -> Result<impl IntoResponse, ApiError> {
    match s.store.get_revision(id).map_err(ApiError::internal)? {
        Some(rev) => Ok(Json(rev)),
        None => Err(ApiError {
            status: StatusCode::NOT_FOUND,
            message: format!("no revision {id}"),
        }),
    }
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn internal(e: anyhow::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: format!("{e:#}"),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, self.message).into_response()
    }
}
