//! `golemd` — the bookkeeping agent.
//!
//! Accepts blueprints over HTTP (commission / decommission), persists
//! them to SQLite, journals every change as a revision. Does not build
//! or tear down anything on the host. That layer comes later.

mod http;
mod store;

use anyhow::{Context, Result};
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "golem bookkeeping agent")]
struct Cli {
    /// This node's name. Returned by /status; for now informational.
    #[arg(long, env = "GOLEM_NODE")]
    node: String,

    /// Where to keep state.db.
    #[arg(long, default_value = "/var/lib/golem")]
    state_dir: PathBuf,

    /// HTTP listen address.
    #[arg(long, default_value = "127.0.0.1:7474")]
    listen: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let cli = Cli::parse();
    std::fs::create_dir_all(&cli.state_dir)
        .with_context(|| format!("create state dir {}", cli.state_dir.display()))?;

    let store = Arc::new(store::Store::open(&cli.state_dir.join("state.db"))?);
    let state = http::AppState {
        node: cli.node.clone(),
        store,
    };

    let app = http::router(state);
    let listener = TcpListener::bind(cli.listen)
        .await
        .with_context(|| format!("bind {}", cli.listen))?;

    info!(node = %cli.node, listen = %cli.listen, "golemd ready");
    axum::serve(listener, app).await?;
    Ok(())
}
