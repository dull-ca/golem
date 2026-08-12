use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use golemctl::conn::Conn;
use golemctl::inventory::{Endpoint, Target};
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
        /// Also diff against what is actually on the host, read live.
        #[arg(long)]
        against_host: bool,
    },
    /// Fan a verb out over every host in a TOML inventory, concurrently. One
    /// host's failure never stops the others. See [`golemctl::fleet`] for the
    /// outcome taxonomy and exit codes, [`golemctl::inventory`] for the file.
    Fleet {
        #[command(subcommand)]
        cmd: FleetCmd,
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

#[derive(Args, Debug)]
struct InventorySelection {
    /// Inventory path; otherwise $GOLEMCTL_INVENTORY, otherwise ./fleet.toml
    #[arg(long, global = true)]
    inventory: Option<PathBuf>,
    /// Apply to this comma-separated subset of the inventory's hosts
    #[arg(long, global = true, value_name = "a,b")]
    hosts: Option<String>,
}

#[derive(Subcommand, Debug)]
enum FleetCmd {
    /// Compile once, fire every host's reconcile concurrently, follow them all.
    /// A host the manifest names no scroll for is skipped untouched. Exits 0
    /// only if every host settled or was skipped.
    Apply {
        source: PathBuf,
        #[command(flatten)]
        selection: InventorySelection,
        /// Emit one aggregate JSON object on stdout, no TUI (also the non-TTY path)
        #[arg(long)]
        json: bool,
    },
    /// Ask every host what an apply would do, changing nothing.
    Plan {
        source: PathBuf,
        #[command(flatten)]
        selection: InventorySelection,
        /// Emit the per-host plan responses as one JSON object
        #[arg(long)]
        json: bool,
        /// Expand every group to one glyph per line, with content ids
        #[arg(long)]
        detail: bool,
        #[arg(long)]
        against_host: bool,
    },
    /// One marked line per inventory host: latest revision, applied content id.
    Status {
        #[command(flatten)]
        selection: InventorySelection,
        /// Emit the per-host readings as one JSON object
        #[arg(long)]
        json: bool,
    },
}

impl InventorySelection {
    // Every fleet verb resolves its targets before compiling the source, so a
    // missing inventory or a typo'd `--hosts` name fails while nothing has been
    // built and no daemon has been contacted.
    fn targets(self) -> Result<Vec<golemctl::inventory::Target>> {
        let path = golemctl::inventory::resolve(self.inventory);
        golemctl::inventory::load(&path)?.select(self.hosts.as_deref())
    }
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
            let conn = connect(&addr).await?;
            golemctl::apply::run(bytes, conn, json, reattach).await
        }
        Cmd::Plan {
            source,
            addr,
            json,
            detail,
            against_host,
        } => {
            let bytes = manifest_bytes(&source).await?;
            let conn = connect(&addr).await?;
            golemctl::plan::run(bytes, &conn, json, detail, against_host).await
        }
        Cmd::Fleet { cmd } => match cmd {
            FleetCmd::Apply {
                source,
                selection,
                json,
            } => {
                let targets = selection.targets()?;
                let bytes = manifest_bytes(&source).await?;
                golemctl::fleet::run_apply(bytes, targets, json).await
            }
            FleetCmd::Plan {
                source,
                selection,
                json,
                detail,
                against_host,
            } => {
                let targets = selection.targets()?;
                let bytes = manifest_bytes(&source).await?;
                golemctl::fleet::run_plan(bytes, targets, json, detail, against_host).await
            }
            FleetCmd::Status { selection, json } => {
                let targets = selection.targets()?;
                golemctl::fleet::run_status(targets, json).await
            }
        },
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
    let conn = connect(addr).await?;
    let value = conn.get_json(path).await?;
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

async fn connect(addr: &str) -> Result<Conn> {
    let auth = golemctl::conn::resolve_auth(None)?;
    let target = Target {
        name: addr.to_string(),
        endpoint: Endpoint::parse(addr)?,
        token_file: None,
    };
    Conn::open(&target, &auth).await
}
