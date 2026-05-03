//! golemd — the agent.

mod bundle;
mod deps;
mod http;
mod providers;
mod reconcile;
mod state;

use anyhow::{Context, Result};
use clap::Parser;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "golem fleet agent")]
struct Cli {
    /// Where to keep state.db and any future scratch files.
    #[arg(long, default_value = "/var/lib/golem")]
    state_dir: PathBuf,

    /// Optional bundle file to load on startup (skips waiting for HTTP push).
    #[arg(long)]
    bundle: Option<PathBuf>,

    /// File containing newline-separated trusted ed25519 public keys (hex).
    #[arg(long, default_value = "/etc/golem/trusted-keys")]
    trusted_keys: PathBuf,

    /// This node's name. Must match `node` in incoming bundles.
    #[arg(long, env = "GOLEM_NODE")]
    node: String,

    /// HTTP listen address.
    #[arg(long, default_value = "127.0.0.1:7474")]
    listen: SocketAddr,

    /// Reconcile period in seconds.
    #[arg(long, default_value_t = 30)]
    period_secs: u64,
}

fn load_trusted_keys(path: &std::path::Path) -> Result<HashSet<String>> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?;
    Ok(s.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,golemd=debug")))
        .init();

    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.state_dir).ok();

    let store = Arc::new(state::Store::open(&cli.state_dir.join("state.db"))?);
    let bundle = Arc::new(RwLock::new(None));

    // Optional disk-loaded initial bundle (great for bootstrap before HTTP works).
    if let Some(path) = &cli.bundle {
        let trust = bundle::TrustConfig {
            node_name:    cli.node.clone(),
            trusted_keys: load_trusted_keys(&cli.trusted_keys)?,
        };
        let body = std::fs::read(path)
            .with_context(|| format!("read bundle {}", path.display()))?;
        match bundle::load_signed(&body, &trust, None) {
            Ok(b) => {
                info!("loaded initial bundle version={} from {}", b.version, path.display());
                *bundle.write().await = Some(b);
            }
            Err(e) => {
                tracing::warn!("could not load initial bundle: {e:#}");
            }
        }
    }

    let trust = Arc::new(bundle::TrustConfig {
        node_name:    cli.node.clone(),
        trusted_keys: load_trusted_keys(&cli.trusted_keys)?,
    });

    let app_state = http::AppState {
        trust:  trust.clone(),
        bundle: bundle.clone(),
    };

    let app = http::router(app_state);
    let listener = TcpListener::bind(cli.listen).await
        .with_context(|| format!("bind {}", cli.listen))?;
    info!("listening on http://{}", cli.listen);

    // Reconciler in its own task.
    let reconciler = reconcile::Reconciler::new(store.clone(), bundle.clone());
    let period = Duration::from_secs(cli.period_secs);
    let recon_task = tokio::spawn(async move { reconciler.run_forever(period).await });

    // HTTP server in main task.
    let serve_task = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            tracing::error!("axum: {e:#}");
        }
    });

    tokio::select! {
        _ = recon_task => tracing::error!("reconciler exited"),
        _ = serve_task => tracing::error!("http exited"),
        _ = tokio::signal::ctrl_c() => info!("ctrl-c, shutting down"),
    }
    Ok(())
}
