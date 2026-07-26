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
    Apply {
        source: PathBuf,
        addr: String,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        reattach: bool,
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
