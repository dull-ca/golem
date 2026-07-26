//! The golemd binary: parse the CLI, open the plan room, pick a reconciler, and
//! serve the HTTP API until shut down.

use anyhow::{Context, Result};
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
    #[arg(long, default_value = "127.0.0.1:7474")]
    listen: SocketAddr,
    #[arg(long, value_enum, default_value_t = ReconcilerKind::Fake, env = "GOLEM_RECONCILER")]
    reconciler: ReconcilerKind,
    /// Path to a non-default golemd.toml. Absent, the built-in retry defaults
    /// apply (`config::load`).
    #[arg(long)]
    config: Option<PathBuf>,
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
    let retry =
        golemd::config::load(cli.config.as_deref()).with_context(|| "load golemd config")?;
    let foreman = Arc::new(
        Foreman::new(cli.host.clone(), Box::new(planroom), reconciler).with_retry_config(retry),
    );

    let app = http::router(http::AppState { foreman });
    let listener = TcpListener::bind(cli.listen)
        .await
        .with_context(|| format!("bind {}", cli.listen))?;
    info!(host = %cli.host, listen = %cli.listen, "golemd ready");
    axum::serve(listener, app).await?;
    Ok(())
}
