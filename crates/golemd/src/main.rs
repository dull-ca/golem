//! `golemd` — a golem: a foreman directing a builder, serving its plan room
//! over HTTP.

use anyhow::{Context, Result};
use clap::Parser;
use golemd::builder::RandomBuilder;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::planroom::SqlitePlanRoom;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "golem agent")]
struct Cli {
    /// This golem's host. Actions for this host are built locally.
    #[arg(long, env = "GOLEM_HOST")]
    host: String,
    /// Directory holding the plan room database.
    #[arg(long, default_value = "/var/lib/golem")]
    state_dir: PathBuf,
    /// Address to serve the HTTP API on.
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
        .with_context(|| format!("create {}", cli.state_dir.display()))?;

    let planroom = SqlitePlanRoom::open(&cli.state_dir.join("planroom.db"))?;
    let foreman = Arc::new(Foreman::new(
        cli.host.clone(),
        Box::new(planroom),
        Box::new(RandomBuilder::default()),
    ));

    let app = http::router(http::AppState { foreman });
    let listener = TcpListener::bind(cli.listen)
        .await
        .with_context(|| format!("bind {}", cli.listen))?;
    info!(host = %cli.host, listen = %cli.listen, "golemd ready");
    axum::serve(listener, app).await?;
    Ok(())
}
