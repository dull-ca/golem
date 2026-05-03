//! golemctl — operator CLI.
//!
//! Subcommands:
//!   keygen                            — emit an ed25519 keypair
//!   eval <config.ncl> <node>          — evaluate Fleet, dump bundle JSON to stdout
//!   sign <bundle.json> <secret_key>   — wrap into SignedBundle JSON
//!   push <signed.json> <addr>         — POST to a node's /bundle
//!   apply <config.ncl>                — eval+sign+push for every node in the fleet

use anyhow::{anyhow, bail, Context, Result};
use clap::{Parser, Subcommand};
use ed25519_dalek::{Signer, SigningKey};
use golem_types::{canonical_json, Bundle, SignedBundle};
use rand_core::OsRng;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Parser, Debug)]
#[command(version, about = "golem fleet operator CLI")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Generate a new ed25519 keypair. Writes <name>.sk and <name>.pk.
    Keygen { name: PathBuf },

    /// Evaluate a Nickel Fleet config for a specific node, dump Bundle JSON.
    Eval {
        config: PathBuf,
        node:   String,
    },

    /// Sign a Bundle JSON into a SignedBundle JSON.
    Sign {
        bundle:     PathBuf,
        secret_key: PathBuf,
    },

    /// POST a SignedBundle JSON to a node's /bundle endpoint.
    Push {
        signed: PathBuf,
        addr:   String,    // "http://10.42.0.2:7474"
    },

    /// End-to-end: eval each node in the fleet, sign, and push.
    Apply {
        config:     PathBuf,
        secret_key: PathBuf,
        /// Map of "node_name" -> "http://addr:port", JSON.
        node_addrs: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Keygen { name }                            => keygen(&name),
        Cmd::Eval { config, node }                      => eval_cmd(&config, &node).await,
        Cmd::Sign { bundle, secret_key }                => sign_cmd(&bundle, &secret_key),
        Cmd::Push { signed, addr }                      => push_cmd(&signed, &addr).await,
        Cmd::Apply { config, secret_key, node_addrs }   => apply_cmd(&config, &secret_key, &node_addrs).await,
    }
}

// ─── keygen ──────────────────────────────────────────────────────────────

fn keygen(stem: &Path) -> Result<()> {
    let sk = SigningKey::generate(&mut OsRng);
    let pk = sk.verifying_key();
    let sk_path = stem.with_extension("sk");
    let pk_path = stem.with_extension("pk");
    std::fs::write(&sk_path, hex::encode(sk.to_bytes()) + "\n")?;
    std::fs::write(&pk_path, hex::encode(pk.to_bytes()) + "\n")?;
    let mut perms = std::fs::metadata(&sk_path)?.permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o600);
    std::fs::set_permissions(&sk_path, perms)?;
    println!("wrote {} (mode 0600) and {}", sk_path.display(), pk_path.display());
    println!("public key: {}", hex::encode(pk.to_bytes()));
    Ok(())
}

// ─── eval ────────────────────────────────────────────────────────────────

/// Run `nickel export --format json` on the user config, asking it to emit
/// `(import "config.ncl").bundle_for "<node>"`. We do this by writing a
/// tiny driver expression to a temp file that imports the user config.
async fn eval_for_node(config: &Path, node: &str) -> Result<Bundle> {
    let driver = format!(
        r#"(import "{}").bundle_for "{}""#,
        config.canonicalize()?.display(),
        node
    );
    let tmp = tempfile_with(&driver)?;

    let out = Command::new("nickel")
        .args(["export", "--format", "json"])
        .arg(tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("spawn nickel — is `nickel` on PATH?")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("nickel export failed:\n{err}");
    }
    let bundle: Bundle = serde_json::from_slice(&out.stdout)
        .context("parse nickel output as Bundle")?;
    Ok(bundle)
}

fn tempfile_with(contents: &str) -> Result<tempfile::NamedTempFile> {
    use std::io::Write;
    let mut f = tempfile::Builder::new().suffix(".ncl").tempfile()?;
    write!(f.as_file_mut(), "{contents}")?;
    f.as_file_mut().sync_all()?;
    Ok(f)
}

async fn eval_cmd(config: &Path, node: &str) -> Result<()> {
    let bundle = eval_for_node(config, node).await?;
    println!("{}", serde_json::to_string_pretty(&bundle)?);
    Ok(())
}

// ─── sign ────────────────────────────────────────────────────────────────

fn sign_bundle(bundle: Bundle, sk: &SigningKey) -> Result<SignedBundle> {
    let canonical = canonical_json(&bundle).context("canonicalize bundle for signing")?;
    let sig = sk.sign(&canonical);
    Ok(SignedBundle {
        bundle,
        signer_pk: hex::encode(sk.verifying_key().to_bytes()),
        signature: hex::encode(sig.to_bytes()),
    })
}

fn read_sk(path: &Path) -> Result<SigningKey> {
    let s = std::fs::read_to_string(path)?;
    let bytes: [u8; 32] = hex::decode(s.trim())?
        .try_into()
        .map_err(|_| anyhow!("secret key is not 32 bytes"))?;
    Ok(SigningKey::from_bytes(&bytes))
}

fn sign_cmd(bundle_path: &Path, sk_path: &Path) -> Result<()> {
    let bundle: Bundle = serde_json::from_slice(&std::fs::read(bundle_path)?)?;
    let sk = read_sk(sk_path)?;
    let signed = sign_bundle(bundle, &sk)?;
    println!("{}", serde_json::to_string_pretty(&signed)?);
    Ok(())
}

// ─── push ────────────────────────────────────────────────────────────────

async fn push_signed(signed: &SignedBundle, addr: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let url = format!("{}/bundle", addr.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .header("content-type", "application/json")
        .body(serde_json::to_vec(signed)?)
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("{addr} responded {status}: {body}");
    }
    println!("{addr} -> {status} {body}");
    Ok(())
}

async fn push_cmd(signed_path: &Path, addr: &str) -> Result<()> {
    let signed: SignedBundle = serde_json::from_slice(&std::fs::read(signed_path)?)?;
    push_signed(&signed, addr).await
}

// ─── apply ───────────────────────────────────────────────────────────────

async fn apply_cmd(config: &Path, sk_path: &Path, addrs_path: &Path) -> Result<()> {
    let sk = read_sk(sk_path)?;
    let addrs: std::collections::BTreeMap<String, String> =
        serde_json::from_slice(&std::fs::read(addrs_path)?)
            .context("parse node addresses JSON")?;

    // We need the list of nodes — read it from the fleet by evaluating
    // `(import "...").nodes` and pulling keys.
    let driver = format!(
        r#"std.record.fields ((import "{}").nodes)"#,
        config.canonicalize()?.display()
    );
    let tmp = tempfile_with(&driver)?;
    let out = Command::new("nickel")
        .args(["export", "--format", "json"])
        .arg(tmp.path())
        .output()
        .await
        .context("nickel — to enumerate fleet nodes")?;
    if !out.status.success() {
        bail!("nickel export failed:\n{}", String::from_utf8_lossy(&out.stderr));
    }
    let nodes: Vec<String> = serde_json::from_slice(&out.stdout)
        .context("parse node list")?;

    let mut errors = 0usize;
    for node in &nodes {
        let addr = match addrs.get(node) {
            Some(a) => a.clone(),
            None => { eprintln!("no address for node `{node}`, skipping"); continue; }
        };
        match eval_for_node(config, node).await {
            Ok(b) => {
                let signed = sign_bundle(b, &sk)?;
                if let Err(e) = push_signed(&signed, &addr).await {
                    eprintln!("push to {node} ({addr}) failed: {e:#}");
                    errors += 1;
                }
            }
            Err(e) => {
                eprintln!("eval for {node} failed: {e:#}");
                errors += 1;
            }
        }
    }
    if errors > 0 { bail!("{errors} node(s) failed"); }
    Ok(())
}
