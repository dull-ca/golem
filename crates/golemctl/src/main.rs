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
use nickel_lang::{Context as NickelContext, ErrorFormat};
use rand_core::OsRng;
use std::path::{Path, PathBuf};

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
        Cmd::Eval { config, node }                      => eval_cmd(&config, &node),
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

/// Evaluate the user's config via embedded Nickel and deserialize the result
/// for one node. We build a small driver expression that imports the user's
/// config by absolute path and calls `.bundle_for "<node>"`, then hand it to
/// `nickel-lang`'s `Context`. Relative imports inside the user's config
/// (e.g. `import "../../nickel/lib.ncl"`) resolve relative to the user's
/// config file, since Nickel resolves imports relative to the importing file.
fn eval_for_node(config: &Path, node: &str) -> Result<Bundle> {
    let abs = config
        .canonicalize()
        .with_context(|| format!("canonicalize {}", config.display()))?;
    let src = format!(
        r#"(import "{}").bundle_for "{}""#,
        nickel_escape(&abs.to_string_lossy()),
        nickel_escape(node),
    );
    let mut ctx = NickelContext::new().with_source_name("<golemctl-eval>".into());
    let expr = ctx
        .eval_deep_for_export(&src)
        .map_err(|e| anyhow!("nickel eval failed:\n{}", format_nickel_err(&e)))?;
    let json = ctx
        .expr_to_json(&expr)
        .map_err(|e| anyhow!("export nickel result as JSON:\n{}", format_nickel_err(&e)))?;
    serde_json::from_str(&json).context("parse nickel output as Bundle")
}

fn list_nodes(config: &Path) -> Result<Vec<String>> {
    let abs = config
        .canonicalize()
        .with_context(|| format!("canonicalize {}", config.display()))?;
    let src = format!(
        r#"std.record.fields ((import "{}").nodes)"#,
        nickel_escape(&abs.to_string_lossy()),
    );
    let mut ctx = NickelContext::new().with_source_name("<golemctl-nodes>".into());
    let expr = ctx
        .eval_deep_for_export(&src)
        .map_err(|e| anyhow!("nickel eval failed:\n{}", format_nickel_err(&e)))?;
    let json = ctx
        .expr_to_json(&expr)
        .map_err(|e| anyhow!("export nickel result as JSON:\n{}", format_nickel_err(&e)))?;
    serde_json::from_str(&json).context("parse node list")
}

fn nickel_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn format_nickel_err(e: &nickel_lang::Error) -> String {
    let mut buf = Vec::new();
    let _ = e.format(&mut buf, ErrorFormat::Text);
    String::from_utf8_lossy(&buf).into_owned()
}

fn eval_cmd(config: &Path, node: &str) -> Result<()> {
    let bundle = eval_for_node(config, node)?;
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

    let nodes = list_nodes(config)?;

    let mut errors = 0usize;
    for node in &nodes {
        let addr = match addrs.get(node) {
            Some(a) => a.clone(),
            None => { eprintln!("no address for node `{node}`, skipping"); continue; }
        };
        match eval_for_node(config, node) {
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
