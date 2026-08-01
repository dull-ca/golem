//! The golemd binary: parse the CLI, open the plan room, pick a reconciler, and
//! serve the HTTP API until shut down.

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use golemd::fake_reconciler::FakeReconciler;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::planroom::SqlitePlanRoom;
use golemd::reconciler::Reconciler;
use golemd::reconcilers::HostReconciler;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Which reconciler enacts glyphs. `Fake` is the default: it records intent
/// without touching the host, so golemd is safe to run anywhere by default;
/// `Host` selects the real apt/systemd/file adapters and is opted into with
/// `--reconciler host`.
#[derive(Clone, Copy, Debug, ValueEnum)]
enum ReconcilerKind {
    Host,
    Fake,
}

#[derive(Parser, Debug)]
#[command(version, about = "golem agent")]
struct Cli {
    #[arg(long, env = "GOLEM_HOST")]
    host: String,
    #[arg(long, default_value = "/var/lib/golem")]
    state_dir: PathBuf,
    /// Address to serve on. Loopback is the deployed posture (ADR 0042):
    /// operators reach the daemon through an ssh forward, and a routable bind
    /// publishes root-equivalent control of this host.
    #[arg(long, default_value = "127.0.0.1:7474")]
    listen: SocketAddr,
    #[arg(long, value_enum, default_value_t = ReconcilerKind::Fake, env = "GOLEM_RECONCILER")]
    reconciler: ReconcilerKind,
    /// Path to a non-default golemd.toml. Absent, the built-in retry defaults
    /// apply (`config::load`).
    #[arg(long)]
    config: Option<PathBuf>,
    /// File holding the shared secret every request must present as
    /// `Authorization: Bearer <token>`. Overrides `[auth] token_file`. With
    /// neither set golemd answers anyone who reaches the port — dev only.
    #[arg(long)]
    auth_token_file: Option<PathBuf>,
}

/// Read the secret golemd will require, once, at startup — an unreadable or
/// empty file stops the daemon rather than starting it ungated, so a
/// mis-provisioned token can never look like a working deployment. `None` (no
/// flag and no `[auth]` table) is the deliberate ungated posture, the only way
/// to get one. Trailing whitespace is trimmed because the file is written by
/// hand and by shell redirection as often as by the harness.
fn load_required_token(path: Option<PathBuf>) -> Result<Option<Arc<String>>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("read auth token file {}", path.display()))?;
    let trimmed = contents.trim_end();
    if trimmed.is_empty() {
        bail!("auth token file {} is empty", path.display());
    }
    Ok(Some(Arc::new(trimmed.to_string())))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.state_dir)
        .with_context(|| format!("create {}", cli.state_dir.display()))?;

    let planroom = SqlitePlanRoom::open(&cli.state_dir.join("planroom.db"))?;
    let reconciler: Box<dyn Reconciler> = match cli.reconciler {
        ReconcilerKind::Host => Box::new(HostReconciler::system()),
        ReconcilerKind::Fake => Box::new(FakeReconciler::new()),
    };
    // Contain a host-adapter panic at the port so it never unwinds across the
    // foreman's write lock and wedges the daemon (ADR 0033, panic-guard). Tests
    // that simulate a *process crash* build the foreman without this wrap, so an
    // uncaught panic still models a crash for the recovery path (ADR 0020 §3).
    let reconciler: Box<dyn Reconciler> =
        Box::new(golemd::reconciler::PanicCatching::new(reconciler));
    let config =
        golemd::config::load(cli.config.as_deref()).with_context(|| "load golemd config")?;
    let foreman = Arc::new(
        Foreman::new(cli.host.clone(), Box::new(planroom), reconciler)
            .with_retry_config(config.retry)
            .with_enact_config(config.enact),
    );

    let required_token = load_required_token(cli.auth_token_file.or(config.auth.token_file))?;

    let app = http::router(http::AppState {
        foreman,
        required_token,
    });
    let listener = TcpListener::bind(cli.listen)
        .await
        .with_context(|| format!("bind {}", cli.listen))?;
    info!(host = %cli.host, listen = %cli.listen, "golemd ready");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_path_gives_no_required_token() {
        assert!(load_required_token(None).unwrap().is_none());
    }

    #[test]
    fn token_is_read_and_trailing_whitespace_trimmed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "s3cret\n").unwrap();
        let token = load_required_token(Some(path)).unwrap().unwrap();
        assert_eq!(*token, "s3cret");
    }

    #[test]
    fn an_empty_token_file_is_a_startup_error_mentioning_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "\n").unwrap();
        let err = load_required_token(Some(path.clone())).unwrap_err();
        assert!(err.to_string().contains(&path.display().to_string()));
    }

    #[test]
    fn an_unreadable_token_file_is_a_startup_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing-token");
        assert!(load_required_token(Some(path)).is_err());
    }
}
