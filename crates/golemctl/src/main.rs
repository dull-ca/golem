//! `golemctl` — talk to a `golemd` node.
//!
//! Subcommands (any unambiguous prefix works, Mercurial-style — e.g.
//! `comm`, `deco`, `st`, `sh`):
//!
//!   commission   <bp.ncl>  <addr>     evaluate Nickel, POST /blueprints
//!   decommission <name>    <addr>     DELETE /blueprints/:name
//!   state        <addr>               GET /state  (current resolved view)
//!   history      <addr>               GET /revisions (the journal)
//!   show         <addr> <id>          GET /revisions/:id

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use golem_types::Blueprint;
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
    /// Evaluate a Nickel file producing a Blueprint, then commission it.
    Commission { config: PathBuf, addr: String },

    /// Decommission a blueprint by name.
    Decommission { name: String, addr: String },

    /// Print the node's current canonical state.
    State { addr: String },

    /// Print the node's revision journal.
    History { addr: String },

    /// Print one revision in full.
    Show { addr: String, id: u64 },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Commission { config, addr } => commission(&config, &addr).await,
        Cmd::Decommission { name, addr } => decommission(&name, &addr).await,
        Cmd::State { addr } => fetch_and_print(&addr, "state").await,
        Cmd::History { addr } => fetch_and_print(&addr, "revisions").await,
        Cmd::Show { addr, id } => fetch_and_print(&addr, &format!("revisions/{id}")).await,
    }
}

async fn commission(config: &Path, addr: &str) -> Result<()> {
    let bp = eval_blueprint(config).await?;
    let url = format!("{}/blueprints", addr.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&bp)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    print_response(resp).await
}

async fn decommission(name: &str, addr: &str) -> Result<()> {
    let url = format!(
        "{}/blueprints/{}",
        addr.trim_end_matches('/'),
        urlencode(name)
    );
    let resp = reqwest::Client::new()
        .delete(&url)
        .send()
        .await
        .with_context(|| format!("DELETE {url}"))?;
    print_response(resp).await
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
    // Pretty-print JSON when possible; otherwise raw.
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
        Err(_) => println!("{body}"),
    }
    Ok(())
}

/// Evaluate `<config>` with `nickel export --format json`, parse as a
/// Blueprint. The Nickel file is expected to produce a record matching
/// `g.Blueprint`.
async fn eval_blueprint(config: &Path) -> Result<Blueprint> {
    let out = Command::new("nickel")
        .args(["export", "--format", "json"])
        .arg(config)
        .output()
        .await
        .context("spawn nickel — is `nickel` on PATH?")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("nickel export failed:\n{err}");
    }
    serde_json::from_slice::<Blueprint>(&out.stdout)
        .with_context(|| format!("parse nickel output as Blueprint ({})", config.display()))
}

/// Minimal percent-encode for path segments. Good enough for blueprint
/// names that are normal identifiers.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
