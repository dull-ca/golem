//! Apt package provider.
//!
//! The honest-cleanup discipline shows up here: if a human already
//! `apt install`ed caddy, capture records `preexisting=true` and unapply
//! never removes it. If we installed it (capture said preexisting=false),
//! unapply autoremoves once the refcount hits zero.

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use golem_types::{AptPackageSpec, Backup, Capture, CaptureError, ClaimSpec, Health};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::Mutex;

use super::{Observation, Provider};

/// Process-wide lock. apt/dpkg serialize their own operations, but if we
/// fan out claims in parallel we'd collect "dpkg frontend locked" errors.
static APT_LOCK: std::sync::OnceLock<Arc<Mutex<()>>> = std::sync::OnceLock::new();

pub struct AptProvider {
    lock: Arc<Mutex<()>>,
}

impl AptProvider {
    pub fn global() -> Self {
        let lock = APT_LOCK.get_or_init(|| Arc::new(Mutex::new(()))).clone();
        Self { lock }
    }
}

fn spec_of(spec: &ClaimSpec) -> &AptPackageSpec {
    match spec {
        ClaimSpec::AptPackage(a) => a,
        _ => unreachable!("AptProvider dispatched on non-apt spec"),
    }
}

async fn dpkg_query(pkg: &str) -> Result<Option<String>> {
    let out = Command::new("dpkg-query")
        .args(["-W", "-f=${Status}\t${Version}", pkg])
        .output()
        .await?;
    if !out.status.success() {
        return Ok(None);
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let mut parts = s.splitn(2, '\t');
    let status = parts.next().unwrap_or("");
    let version = parts.next().unwrap_or("").to_string();
    if status.contains("install ok installed") {
        Ok(Some(version))
    } else {
        Ok(None)
    }
}

async fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .env("DEBIAN_FRONTEND", "noninteractive")
        .status()
        .await
        .with_context(|| format!("spawn {cmd}"))?;
    if !status.success() {
        bail!("{cmd} {args:?} failed: {status}");
    }
    Ok(())
}

#[async_trait]
impl Provider for AptProvider {
    async fn observe(&self, spec: &ClaimSpec) -> Result<Observation> {
        let s = spec_of(spec);
        match dpkg_query(&s.name).await? {
            Some(_) => Ok(Observation::Present),
            None => Ok(Observation::Absent),
        }
    }

    async fn matches(&self, spec: &ClaimSpec) -> Result<bool> {
        let s = spec_of(spec);
        match dpkg_query(&s.name).await? {
            None => Ok(false),
            Some(installed_ver) => Ok(match &s.version {
                None => true,
                Some(want) => installed_ver == *want,
            }),
        }
    }

    async fn capture(&self, spec: &ClaimSpec) -> Result<Capture, CaptureError> {
        let s = spec_of(spec);
        let installed = dpkg_query(&s.name)
            .await
            .map_err(|e| CaptureError::Other(anyhow!(e)))?
            .is_some();
        Ok(Capture {
            preexisting: installed,
            backup: Backup { existed: installed, ..Backup::default() },
        })
    }

    async fn mutate(&self, spec: &ClaimSpec, capture: &Capture) -> Result<()> {
        let s = spec_of(spec);
        let _guard = self.lock.lock().await;

        if capture.preexisting {
            // Package already there by some other hand. Don't touch version
            // unless explicitly pinned — upgrading other people's packages
            // would be a surprise.
            if let Some(want) = &s.version {
                if dpkg_query(&s.name).await? != Some(want.clone()) {
                    let pkg = format!("{}={}", s.name, want);
                    run("apt-get", &["install", "-y", "--only-upgrade", &pkg]).await?;
                }
            }
        } else {
            let pkg = match &s.version {
                Some(v) => format!("{}={}", s.name, v),
                None => s.name.clone(),
            };
            // apt-get update failure is fatal: a stale cache silently installs
            // outdated versions. Surface it instead of swallowing.
            run("apt-get", &["update"]).await.context("apt-get update")?;
            run(
                "apt-get",
                &["install", "-y", "--no-install-recommends", &pkg],
            )
            .await?;
        }

        if s.hold {
            run("apt-mark", &["hold", &s.name]).await?;
        }
        Ok(())
    }

    async fn unmutate(&self, spec: &ClaimSpec, capture: &Capture) -> Result<()> {
        let s = spec_of(spec);
        let _guard = self.lock.lock().await;

        if capture.preexisting {
            return Ok(()); // Never remove what we didn't install.
        }
        if s.hold {
            run("apt-mark", &["unhold", &s.name]).await.ok();
        }
        run("apt-get", &["autoremove", "-y", "--purge", &s.name]).await?;
        Ok(())
    }

    async fn check(&self, spec: &ClaimSpec) -> Result<Health> {
        if self.matches(spec).await? {
            Ok(Health::Healthy)
        } else {
            Ok(Health::Degraded("apt: not installed or wrong version".into()))
        }
    }
}
