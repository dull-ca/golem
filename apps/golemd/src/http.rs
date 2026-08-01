//! The HTTP adapter over the foreman (ADR 0014 §5). `POST /manifest` ingests
//! raw manifest bytes and starts a reconcile, returning `202 { reconcile_id }`;
//! the client then polls `GET /reconciles/<id>|latest` until the projection
//! settles (the two-request protocol, ADR 0033 §1–2). The remaining read routes
//! expose the current applied scroll (`GET /state`), the journal
//! (`GET /revisions[/:id]`), and a liveness summary (`GET /status`). There is no
//! decommission verb — a node's state is a whole scroll, so "remove everything"
//! is applying an empty one. Foreman calls are blocking, so each runs on
//! `spawn_blocking`.
//!
//! Every route sits behind one authorization check when
//! `AppState::required_token` carries a secret: `Authorization: Bearer <token>`
//! or a typed `401` (ADR 0042). It is deliberately the whole of golemd's
//! security model — no per-user identity, no signing, no TLS — because a
//! deployed daemon binds loopback and is reached through the operator's ssh
//! tunnel, so the header is what distinguishes "can log into this box" from
//! "may submit changes to it". No token configured is no gate at all: the
//! dev/test posture ADR 0040 describes, never a deployed one. Swapping the
//! shared secret for an authentik-issued token later changes
//! `require_bearer` and nothing else.

use axum::{
    body::Bytes,
    extract::{Path, Query, Request, State as AxState},
    http::{header::AUTHORIZATION, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::foreman::Foreman;
use crate::journal::AppliedState;

/// What every handler shares: the foreman, and the secret a caller must present
/// if one is configured. `None` is an ungated daemon.
#[derive(Clone)]
pub struct AppState {
    pub foreman: Arc<Foreman>,
    pub required_token: Option<Arc<String>>,
}

/// Wire the routes to the shared foreman state. `/reconciles/latest` is
/// registered before `/reconciles/:id` so `latest` is not captured as an id.
///
/// The gate layer is added only when a token is configured, so an ungated
/// daemon serves a router with no auth middleware in it at all rather than one
/// whose middleware waves every request through. Both routers answer
/// identically apart from the check — a gated route is never a different route.
pub fn router(app: AppState) -> Router {
    let gate_state = app.clone();
    let router = Router::new()
        .route("/manifest", post(apply_manifest))
        .route("/plan", post(plan_manifest))
        .route("/reconciles/latest", get(reconcile_latest))
        .route("/reconciles/:id", get(reconcile))
        .route("/state", get(state))
        .route("/revisions", get(revisions))
        .route("/revisions/:id", get(revision))
        .route("/status", get(status))
        .with_state(app);
    if gate_state.required_token.is_some() {
        router.layer(from_fn_with_state(gate_state, require_bearer))
    } else {
        router
    }
}

/// Compare in time that depends on the token's length but not its bytes: the
/// fold reads every byte whatever the first mismatch is, so a caller cannot
/// learn the secret one character at a time from response timing. Written by
/// hand rather than pulled from a crate — it is five lines, and golem takes no
/// dependency for it.
///
/// NOTE: the length guard leaks the secret's length before any comparison
/// runs. That is accepted (ADR 0042): the token is 32 random bytes from the
/// harness's `secrets.token_urlsafe`, and knowing how long it is buys an
/// attacker nothing they could not already assume.
fn token_matches(presented: &str, required: &str) -> bool {
    let (p, r) = (presented.as_bytes(), required.as_bytes());
    if p.len() != r.len() {
        return false;
    }
    p.iter().zip(r).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
}

/// Admit a request only when it presents the configured secret. A malformed,
/// non-`Bearer`, absent, or wrong header is one and the same `401` — the reply
/// says what to set, never which part of the attempt was wrong.
///
/// NOTE: the `required_token`-is-`None` arm is unreachable through [`router`],
/// which layers this middleware on only when a token exists — mounting it by
/// hand against a tokenless state would wave every request through.
async fn require_bearer(
    AxState(state): AxState<AppState>,
    req: Request,
    next: Next,
) -> axum::response::Response {
    let Some(required) = &state.required_token else {
        return next.run(req).await;
    };
    let presented = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(token) if token_matches(token, required) => next.run(req).await,
        _ => ApiError::unauthorized().into_response(),
    }
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
    tokio::task::spawn_blocking(move || foreman.run_reconcile_guarded(reconcile_id, selected));
    Ok((StatusCode::ACCEPTED, Json(Accepted { reconcile_id })))
}

async fn plan_manifest(
    AxState(s): AxState<AppState>,
    body: Bytes,
) -> Result<impl IntoResponse, ApiError> {
    let bytes = body.to_vec();
    let foreman = s.foreman.clone();
    let report = tokio::task::spawn_blocking(move || foreman.plan_manifest(&bytes))
        .await
        .map_err(|e| ApiError::internal(anyhow::anyhow!("task join: {e}")))?
        .map_err(ApiError::from_foreman)?;
    Ok(Json(report))
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
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            kind: "unauthorized".to_string(),
            message: "missing or invalid bearer token — golemd requires Authorization: Bearer <token> (see --auth-token-file)".to_string(),
            reconcile_id: None,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(self)).into_response()
    }
}
