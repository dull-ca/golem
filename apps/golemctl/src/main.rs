use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use tokio::process::Command;

#[derive(Parser, Debug)]
#[command(version, about = "golem CLI", infer_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Compile (or read) a manifest, fire it at golemd, and follow the
    /// reconcile live. See [`golemctl::apply`] for the surface and exit codes.
    Apply {
        source: PathBuf,
        addr: String,
        /// Emit the report as JSON, no TUI (also the non-TTY path). The `logs:
        /// <dir>` line and per-event plain lines go to stderr, so stdout is
        /// exactly the final report object — safe to pipe straight into a
        /// JSON consumer.
        #[arg(long)]
        json: bool,
        /// Skip the POST and resume the newest attempt via `/reconciles/latest`.
        #[arg(long)]
        reattach: bool,
    },
    /// Ask golemd what an apply of this manifest would do, changing nothing.
    /// See [`golemctl::plan`] for the rendering and the color policy.
    Plan {
        source: PathBuf,
        addr: String,
        /// Emit golemd's plan response verbatim instead of the collapsed view.
        #[arg(long)]
        json: bool,
        /// Expand every group to one glyph per line, with content ids.
        #[arg(long)]
        detail: bool,
    },
    State {
        addr: String,
    },
    History {
        addr: String,
    },
    Show {
        addr: String,
        id: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Apply {
            source,
            addr,
            json,
            reattach,
        } => {
            let bytes = if reattach {
                Vec::new()
            } else {
                manifest_bytes(&source).await?
            };
            golemctl::apply::run(bytes, &addr, json, reattach).await
        }
        Cmd::Plan {
            source,
            addr,
            json,
            detail,
        } => {
            let bytes = manifest_bytes(&source).await?;
            golemctl::plan::run(bytes, &addr, json, detail).await
        }
        Cmd::State { addr } => fetch_and_print(&addr, "state").await,
        Cmd::History { addr } => fetch_and_print(&addr, "revisions").await,
        Cmd::Show { addr, id } => fetch_and_print(&addr, &format!("revisions/{id}")).await,
    }
}

async fn manifest_bytes(source: &Path) -> Result<Vec<u8>> {
    match source.extension().and_then(|e| e.to_str()) {
        Some("emet") => compile_emet(source).await,
        _ => tokio::fs::read(source)
            .await
            .with_context(|| format!("read manifest {}", source.display())),
    }
}

async fn compile_emet(source: &Path) -> Result<Vec<u8>> {
    let out = Command::new("emetc")
        .arg("build")
        .arg(source)
        .output()
        .await
        .context("spawn emetc — is `emetc` on PATH?")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("emetc build failed:\n{err}");
    }
    Ok(out.stdout)
}

async fn fetch_and_print(addr: &str, path: &str) -> Result<()> {
    let url = format!("{}/{}", addr.trim_end_matches('/'), path);
    let resp = reqwest::get(&url)
        .await
        .with_context(|| format!("GET {url}"))?;
    print_response(resp).await
}

async fn print_response(resp: reqwest::Response) -> Result<()> {
    let status = resp.status();
    let body = resp.text().await?;
    if !status.is_success() {
        bail!("{}: {}", status, body);
    }
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
        Err(_) => println!("{body}"),
    }
    Ok(())
}
