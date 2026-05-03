//! File provider — owns a path, with atomic writes and honest unapply.
//!
//! Trait split: `capture` snapshots prior content/mode read-only, refusing
//! files larger than `MAX_CAPTURE_BYTES`. `mutate` does the atomic write
//! only; it never touches engine state. The reconciler journals the capture
//! before calling mutate.

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use golem_types::{Backup, Capture, CaptureError, ClaimSpec, FileMarker, FileSpec, Health, MAX_CAPTURE_BYTES};
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use super::{Observation, Provider};

pub struct FileProvider;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn spec_of(spec: &ClaimSpec) -> &FileSpec {
    match spec {
        ClaimSpec::File(f) => f,
        _ => unreachable!("FileProvider dispatched on non-File spec"),
    }
}

async fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let dir = path.parent().context("file has no parent dir")?;
    fs::create_dir_all(dir).await.ok();
    let tmp = tempfile::Builder::new()
        .prefix(".golem.")
        .suffix(".tmp")
        .tempfile_in(dir)?;
    let (std_file, tmp_path) = tmp.into_parts();
    {
        let mut f = tokio::fs::File::from_std(std_file);
        f.write_all(bytes).await?;
        f.sync_all().await?;
    }
    let mut perms = std::fs::metadata(&tmp_path)?.permissions();
    perms.set_mode(mode);
    std::fs::set_permissions(&tmp_path, perms)?;
    std::fs::rename(&tmp_path, path)?;
    if let Ok(dirfd) = std::fs::File::open(dir) {
        let _ = dirfd.sync_all();
    }
    Ok(())
}

#[async_trait]
impl Provider for FileProvider {
    async fn observe(&self, spec: &ClaimSpec) -> Result<Observation> {
        let s = spec_of(spec);
        match fs::metadata(&s.path).await {
            Ok(_) => Ok(Observation::Present),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Observation::Absent),
            Err(e) => Err(e.into()),
        }
    }

    async fn matches(&self, spec: &ClaimSpec) -> Result<bool> {
        let s = spec_of(spec);
        match fs::read(&s.path).await {
            Ok(bytes) => {
                let cur_hash = sha256_hex(&bytes);
                let want_hash = sha256_hex(s.content.as_bytes());
                if cur_hash != want_hash { return Ok(false); }
                let meta = fs::metadata(&s.path).await?;
                let cur_mode = meta.permissions().mode() & 0o7777;
                Ok(cur_mode == s.mode)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    async fn capture(&self, spec: &ClaimSpec) -> Result<Capture, CaptureError> {
        let s = spec_of(spec);
        let path = Path::new(&s.path);

        // Stat first — if oversize, refuse before reading the bytes.
        let meta = match fs::metadata(path).await {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Capture {
                    preexisting: false,
                    backup:      Backup { existed: false, ..Backup::default() },
                });
            }
            Err(e) => return Err(CaptureError::Other(e.into())),
        };
        let len = meta.len() as usize;
        if len > MAX_CAPTURE_BYTES {
            return Err(CaptureError::TooLarge(len));
        }

        let bytes = fs::read(path)
            .await
            .map_err(|e| CaptureError::Other(anyhow!(e)))?;
        let mode = meta.permissions().mode() & 0o7777;

        Ok(Capture {
            preexisting: true,
            backup: Backup {
                existed:       true,
                prior_content: Some(B64.encode(&bytes)),
                prior_hash:    Some(sha256_hex(&bytes)),
                prior_mode:    Some(mode),
                ..Backup::default()
            },
        })
    }

    async fn mutate(&self, spec: &ClaimSpec, _capture: &Capture) -> Result<()> {
        let s = spec_of(spec);
        let path = Path::new(&s.path);
        match s.marker {
            FileMarker::Owned | FileMarker::Dropin => {
                write_atomic(path, s.content.as_bytes(), s.mode).await?;
            }
            FileMarker::BlockInFile => {
                anyhow::bail!("BlockInFile marker not yet implemented");
            }
        }
        // TODO: chown to spec.owner/spec.group via nix::unistd::chown.
        Ok(())
    }

    async fn unmutate(&self, spec: &ClaimSpec, capture: &Capture) -> Result<()> {
        let s = spec_of(spec);
        let path = Path::new(&s.path);

        if capture.preexisting {
            if let (Some(b64), Some(mode)) =
                (&capture.backup.prior_content, capture.backup.prior_mode)
            {
                let bytes = B64.decode(b64).context("decode prior content")?;
                write_atomic(path, &bytes, mode).await?;
            }
            return Ok(());
        }

        // We installed it. Remove it.
        match fs::remove_file(path).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    async fn check(&self, spec: &ClaimSpec) -> Result<Health> {
        if self.matches(spec).await? {
            Ok(Health::Healthy)
        } else {
            Ok(Health::Degraded("file drift".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_spec(path: &std::path::Path) -> ClaimSpec {
        ClaimSpec::File(FileSpec {
            path:    path.to_string_lossy().to_string(),
            content: String::new(),
            mode:    0o644,
            owner:   "root".into(),
            group:   "root".into(),
            marker:  FileMarker::Owned,
        })
    }

    /// Capturing a missing file is honest: preexisting=false, no backup blob.
    #[tokio::test]
    async fn capture_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist");
        let cap = FileProvider.capture(&file_spec(&path)).await.unwrap();
        assert!(!cap.preexisting);
        assert!(!cap.backup.existed);
        assert!(cap.backup.prior_content.is_none());
    }

    /// Capturing a small existing file records its content + hash + mode.
    #[tokio::test]
    async fn capture_small_file_records_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("small.txt");
        std::fs::write(&path, b"hello golem").unwrap();
        let cap = FileProvider.capture(&file_spec(&path)).await.unwrap();
        assert!(cap.preexisting);
        assert!(cap.backup.existed);
        assert!(cap.backup.prior_content.is_some());
        assert!(cap.backup.prior_hash.is_some());
        assert!(cap.backup.prior_mode.is_some());
    }

    /// A file whose prior state exceeds MAX_CAPTURE_BYTES must be refused
    /// at capture time. Without this guard the agent would OOM trying to
    /// base64-encode a multi-GB log file. See DESIGN.md §6.
    #[tokio::test]
    async fn capture_refuses_oversized_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.bin");
        // One byte over the cap is enough — the stat check fires before any
        // bytes are read into memory.
        let bytes = vec![0u8; MAX_CAPTURE_BYTES + 1];
        std::fs::write(&path, &bytes).unwrap();

        let err = FileProvider
            .capture(&file_spec(&path))
            .await
            .expect_err("must refuse capture");
        match err {
            CaptureError::TooLarge(n) => assert_eq!(n, MAX_CAPTURE_BYTES + 1),
            other => panic!("expected TooLarge, got {other:?}"),
        }
    }

    /// Right at the cap is still allowed.
    #[tokio::test]
    async fn capture_allows_file_exactly_at_cap() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("limit.bin");
        let bytes = vec![0u8; MAX_CAPTURE_BYTES];
        std::fs::write(&path, &bytes).unwrap();
        let cap = FileProvider.capture(&file_spec(&path)).await.unwrap();
        assert!(cap.preexisting);
    }
}
