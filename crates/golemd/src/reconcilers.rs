//! The real host reconcilers for the four glyphs (ADR 0015 §4). Each `apply`
//! first observes the host so re-applying a matching glyph is a no-op, then
//! captures the exact prior state as an [`Inverse`] so `reverse` restores it —
//! never removing a package, unit, file, or line the host already had. apt and
//! systemd go through the [`CommandRunner`] port; file and lineInFile do real
//! filesystem I/O (atomic temp-file-and-rename for writes) and are tested
//! against tempfiles.
//!
//! Two limits stand for now (ADR 0015):
//! - The `file` inverse holds the prior contents **inline** and reads them as a
//!   `String`, so a non-UTF-8 prior file is a `Fatal` error rather than a
//!   restorable inverse. Binary files and out-of-line blob storage are deferred.
//! - These adapters are exercised via the fake `CommandRunner` and tempfiles;
//!   the end-to-end run against a real Debian box (install → upgrade →
//!   decommission) is deferred to a later phase.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use scroll_format::{ContentId, Glyph};

use crate::host::{CommandRunner, SystemCommandRunner};
use crate::journal::{GlyphOp, Inverse, Outcome};
use crate::reconciler::{EnactError, EnactResult, Reconciler};

/// Enacts the four glyphs on a real host, driving apt and systemd through a
/// [`CommandRunner`] `R` (the `system()` constructor uses the real one; tests
/// inject the fake).
pub struct HostReconciler<R: CommandRunner> {
    runner: R,
}

impl HostReconciler<SystemCommandRunner> {
    pub fn system() -> Self {
        Self { runner: SystemCommandRunner }
    }
}

impl<R: CommandRunner> HostReconciler<R> {
    pub fn with_runner(runner: R) -> Self {
        Self { runner }
    }

    fn apply_apt(&self, name: &str, cid: ContentId, glyph: &Glyph) -> EnactResult<Outcome> {
        if self.apt_installed(name)? {
            return Ok(outcome(glyph, cid, Inverse::Nothing, false));
        }
        let installed = self.runner.run("apt-get", &["install", "-y", name])?;
        if !installed.succeeded() {
            return Err(EnactError::Retryable(format!("apt-get install {name}: {}", installed.stderr)));
        }
        Ok(outcome(glyph, cid, Inverse::RemoveAptPackage { name: name.to_string() }, true))
    }

    fn apt_installed(&self, name: &str) -> EnactResult<bool> {
        let query = self.runner.run("dpkg-query", &["-W", "-f=${Status}", name])?;
        Ok(query.succeeded() && query.stdout.contains("install ok installed"))
    }

    fn apply_systemd(&self, unit: &str, cid: ContentId, glyph: &Glyph) -> EnactResult<Outcome> {
        let prior_enabled = self.systemd_enabled(unit)?;
        let prior_active = self.systemd_active(unit)?;
        if prior_enabled && prior_active {
            return Ok(outcome(glyph, cid, Inverse::Nothing, false));
        }
        let enabled = self.runner.run("systemctl", &["enable", "--now", unit])?;
        if !enabled.succeeded() {
            return Err(EnactError::Retryable(format!("systemctl enable --now {unit}: {}", enabled.stderr)));
        }
        Ok(outcome(
            glyph,
            cid,
            Inverse::DisableSystemdService { unit: unit.to_string(), prior_enabled, prior_active },
            true,
        ))
    }

    fn systemd_enabled(&self, unit: &str) -> EnactResult<bool> {
        Ok(self.runner.run("systemctl", &["is-enabled", unit])?.succeeded())
    }

    fn systemd_active(&self, unit: &str) -> EnactResult<bool> {
        Ok(self.runner.run("systemctl", &["is-active", unit])?.succeeded())
    }

    fn reverse_apt(&self, name: &str) -> EnactResult<()> {
        let removed = self.runner.run("apt-get", &["remove", "-y", name])?;
        if !removed.succeeded() {
            return Err(EnactError::Retryable(format!("apt-get remove {name}: {}", removed.stderr)));
        }
        Ok(())
    }

    fn reverse_systemd(&self, unit: &str, prior_enabled: bool, prior_active: bool) -> EnactResult<()> {
        if !prior_enabled {
            let disabled = self.runner.run("systemctl", &["disable", "--now", unit])?;
            if !disabled.succeeded() {
                return Err(EnactError::Retryable(format!("systemctl disable --now {unit}: {}", disabled.stderr)));
            }
            return Ok(());
        }
        if !prior_active {
            let stopped = self.runner.run("systemctl", &["stop", unit])?;
            if !stopped.succeeded() {
                return Err(EnactError::Retryable(format!("systemctl stop {unit}: {}", stopped.stderr)));
            }
        }
        Ok(())
    }
}

impl<R: CommandRunner> Reconciler for HostReconciler<R> {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        match glyph {
            Glyph::AptPackage { name } => self.apply_apt(name, cid, glyph),
            Glyph::SystemdService { unit } => self.apply_systemd(unit, cid, glyph),
            Glyph::File { path, contents, mode } => apply_file(path, contents, mode, cid, glyph),
            Glyph::LineInFile { path, line } => apply_line_in_file(path, line, cid, glyph),
        }
    }

    fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
        match &outcome.inverse {
            Inverse::Nothing => Ok(()),
            Inverse::RemoveAptPackage { name } => self.reverse_apt(name),
            Inverse::DisableSystemdService { unit, prior_enabled, prior_active } => {
                self.reverse_systemd(unit, *prior_enabled, *prior_active)
            }
            Inverse::RestoreFile { path, contents, mode } => restore_file(path, contents, mode),
            Inverse::DeleteFile { path } => delete_file(path),
            Inverse::RemoveLineInFile { path, line } => remove_line_in_file(path, line),
        }
    }
}

fn outcome(glyph: &Glyph, cid: ContentId, inverse: Inverse, changed: bool) -> Outcome {
    Outcome { op: GlyphOp::Install { cid, glyph: glyph.clone() }, cid, inverse, changed }
}

fn apply_file(path: &str, contents: &str, mode: &str, cid: ContentId, glyph: &Glyph) -> EnactResult<Outcome> {
    let prior = read_file(path)?;
    if let Some((prior_contents, prior_mode)) = &prior {
        if prior_contents == contents && prior_mode == mode {
            return Ok(outcome(glyph, cid, Inverse::Nothing, false));
        }
    }
    write_file_atomic(path, contents, mode)?;
    let inverse = match prior {
        Some((prior_contents, prior_mode)) => {
            Inverse::RestoreFile { path: path.to_string(), contents: prior_contents, mode: prior_mode }
        }
        None => Inverse::DeleteFile { path: path.to_string() },
    };
    Ok(outcome(glyph, cid, inverse, true))
}

/// The prior `(contents, mode)` of a file, or `None` if it is absent — the
/// state captured for the `file` inverse. Reads contents as a `String`, so a
/// non-UTF-8 prior file is a `Fatal` error (the inline-inverse limit noted in
/// the module doc).
fn read_file(path: &str) -> EnactResult<Option<(String, String)>> {
    match fs::read(path) {
        Ok(bytes) => {
            let contents = String::from_utf8(bytes)
                .map_err(|e| EnactError::Fatal(format!("read {path}: not utf-8: {e}")))?;
            let mode = fs::metadata(path)
                .map_err(|e| EnactError::Retryable(format!("stat {path}: {e}")))?
                .permissions()
                .mode();
            Ok(Some((contents, format!("{:04o}", mode & 0o7777))))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EnactError::Retryable(format!("read {path}: {e}"))),
    }
}

/// Write `contents` to `path` at `mode` atomically: fill a temp file in the
/// target directory, set its permissions, then rename it over the target, so a
/// reader never sees a half-written file.
fn write_file_atomic(path: &str, contents: &str, mode: &str) -> EnactResult<()> {
    let target = Path::new(path);
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|e| EnactError::Retryable(format!("mkdir {}: {e}", dir.display())))?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| EnactError::Retryable(format!("temp in {}: {e}", dir.display())))?;
    temp.write_all(contents.as_bytes())
        .map_err(|e| EnactError::Retryable(format!("write temp for {path}: {e}")))?;
    temp.flush().map_err(|e| EnactError::Retryable(format!("flush temp for {path}: {e}")))?;
    let bits = parse_mode(mode)?;
    fs::set_permissions(temp.path(), fs::Permissions::from_mode(bits))
        .map_err(|e| EnactError::Retryable(format!("chmod temp for {path}: {e}")))?;
    temp.persist(target)
        .map_err(|e| EnactError::Retryable(format!("persist {path}: {e}")))?;
    Ok(())
}

fn parse_mode(mode: &str) -> EnactResult<u32> {
    u32::from_str_radix(mode.trim_start_matches("0o"), 8)
        .map_err(|e| EnactError::Fatal(format!("bad mode {mode}: {e}")))
}

fn restore_file(path: &str, contents: &str, mode: &str) -> EnactResult<()> {
    write_file_atomic(path, contents, mode)
}

fn delete_file(path: &str) -> EnactResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(EnactError::Retryable(format!("remove {path}: {e}"))),
    }
}

fn apply_line_in_file(path: &str, line: &str, cid: ContentId, glyph: &Glyph) -> EnactResult<Outcome> {
    if file_has_line(path, line)? {
        return Ok(outcome(glyph, cid, Inverse::Nothing, false));
    }
    append_line(path, line)?;
    Ok(outcome(glyph, cid, Inverse::RemoveLineInFile { path: path.to_string(), line: line.to_string() }, true))
}

fn file_has_line(path: &str, line: &str) -> EnactResult<bool> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(contents.lines().any(|l| l == line)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(EnactError::Retryable(format!("read {path}: {e}"))),
    }
}

fn append_line(path: &str, line: &str) -> EnactResult<()> {
    let target = Path::new(path);
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir).map_err(|e| EnactError::Retryable(format!("mkdir {}: {e}", dir.display())))?;
    let mut existing = fs::read_to_string(path).unwrap_or_default();
    if !existing.is_empty() && !existing.ends_with('\n') {
        existing.push('\n');
    }
    existing.push_str(line);
    existing.push('\n');
    fs::write(path, existing).map_err(|e| EnactError::Retryable(format!("append {path}: {e}")))?;
    Ok(())
}

/// Remove the **first** occurrence of `line` from the file — the one golem
/// appended — leaving any duplicate the host already had, and preserving the
/// file's trailing-newline shape.
fn remove_line_in_file(path: &str, line: &str) -> EnactResult<()> {
    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(EnactError::Retryable(format!("read {path}: {e}"))),
    };
    let trailing_newline = contents.ends_with('\n');
    let mut removed = false;
    let kept: Vec<&str> = contents
        .lines()
        .filter(|l| {
            if !removed && *l == line {
                removed = true;
                false
            } else {
                true
            }
        })
        .collect();
    let mut rebuilt = kept.join("\n");
    if trailing_newline && !rebuilt.is_empty() {
        rebuilt.push('\n');
    }
    fs::write(path, rebuilt).map_err(|e| EnactError::Retryable(format!("rewrite {path}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::fake::FakeCommandRunner;
    use crate::reconcile::glyph_content_id;

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.into() }
    }

    fn systemd(unit: &str) -> Glyph {
        Glyph::SystemdService { unit: unit.into() }
    }

    fn file_glyph(path: &str, contents: &str, mode: &str) -> Glyph {
        Glyph::File { path: path.into(), contents: contents.into(), mode: mode.into() }
    }

    fn line_glyph(path: &str, line: &str) -> Glyph {
        Glyph::LineInFile { path: path.into(), line: line.into() }
    }

    #[test]
    fn apt_isometry_when_absent_removes_on_reverse() {
        let runner = FakeCommandRunner::new();
        let rec = HostReconciler::with_runner(runner);
        let glyph = apt("nginx");
        let cid = glyph_content_id(&glyph);

        assert!(!runner_of(&rec).is_installed("nginx"));
        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert!(runner_of(&rec).is_installed("nginx"));

        rec.reverse(&outcome).unwrap();
        assert!(!runner_of(&rec).is_installed("nginx"));
    }

    #[test]
    fn apt_isometry_when_present_leaves_it() {
        let runner = FakeCommandRunner::with_installed(&["nginx"]);
        let rec = HostReconciler::with_runner(runner);
        let glyph = apt("nginx");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(!outcome.changed);
        assert_eq!(outcome.inverse, Inverse::Nothing);

        rec.reverse(&outcome).unwrap();
        assert!(runner_of(&rec).is_installed("nginx"));
    }

    #[test]
    fn apt_reapply_same_cid_is_unchanged() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = apt("nginx");
        let cid = glyph_content_id(&glyph);
        assert!(rec.apply(&glyph, cid).unwrap().changed);
        assert!(!rec.apply(&glyph, cid).unwrap().changed);
    }

    #[test]
    fn systemd_isometry_when_disabled_reverts_fully() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::with_service("app", false, false));
        let glyph = systemd("app");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert!(runner_of(&rec).is_enabled("app") && runner_of(&rec).is_active("app"));

        rec.reverse(&outcome).unwrap();
        assert!(!runner_of(&rec).is_enabled("app") && !runner_of(&rec).is_active("app"));
    }

    #[test]
    fn systemd_isometry_when_enabled_active_leaves_it() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::with_service("app", true, true));
        let glyph = systemd("app");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(!outcome.changed);
        assert_eq!(outcome.inverse, Inverse::Nothing);

        rec.reverse(&outcome).unwrap();
        assert!(runner_of(&rec).is_enabled("app") && runner_of(&rec).is_active("app"));
    }

    #[test]
    fn systemd_isometry_when_enabled_but_inactive_restores_inactive() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::with_service("app", true, false));
        let glyph = systemd("app");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert!(runner_of(&rec).is_enabled("app") && runner_of(&rec).is_active("app"));

        rec.reverse(&outcome).unwrap();
        assert!(runner_of(&rec).is_enabled("app"));
        assert!(!runner_of(&rec).is_active("app"));
    }

    fn runner_of<R: CommandRunner>(rec: &HostReconciler<R>) -> &R {
        &rec.runner
    }

    #[test]
    fn file_isometry_when_absent_deletes_on_reverse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.conf");
        let path = path.to_str().unwrap();
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = file_glyph(path, "desired\n", "0644");
        let cid = glyph_content_id(&glyph);

        assert!(!Path::new(path).exists());
        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert_eq!(fs::read_to_string(path).unwrap(), "desired\n");

        rec.reverse(&outcome).unwrap();
        assert!(!Path::new(path).exists());
    }

    #[test]
    fn file_isometry_when_present_restores_prior_bytes_and_mode() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.conf");
        let path = path.to_str().unwrap();
        fs::write(path, "original\n").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();

        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = file_glyph(path, "desired\n", "0644");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert_eq!(fs::read_to_string(path).unwrap(), "desired\n");
        assert_eq!(mode_of(path), 0o644);

        rec.reverse(&outcome).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "original\n");
        assert_eq!(mode_of(path), 0o600);
    }

    #[test]
    fn file_reapply_same_contents_and_mode_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.conf");
        let path = path.to_str().unwrap();
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = file_glyph(path, "desired\n", "0644");
        let cid = glyph_content_id(&glyph);

        assert!(rec.apply(&glyph, cid).unwrap().changed);
        assert!(!rec.apply(&glyph, cid).unwrap().changed);
    }

    fn mode_of(path: &str) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn line_isometry_when_absent_removes_on_reverse() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts");
        let path = path.to_str().unwrap();
        fs::write(path, "127.0.0.1 localhost\n").unwrap();

        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = line_glyph(path, "10.0.0.1 app");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert_eq!(fs::read_to_string(path).unwrap(), "127.0.0.1 localhost\n10.0.0.1 app\n");

        rec.reverse(&outcome).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "127.0.0.1 localhost\n");
    }

    #[test]
    fn line_isometry_when_present_leaves_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts");
        let path = path.to_str().unwrap();
        fs::write(path, "127.0.0.1 localhost\n10.0.0.1 app\n").unwrap();

        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = line_glyph(path, "10.0.0.1 app");
        let cid = glyph_content_id(&glyph);

        let before = fs::read_to_string(path).unwrap();
        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(!outcome.changed);
        assert_eq!(outcome.inverse, Inverse::Nothing);

        rec.reverse(&outcome).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), before);
    }

    #[test]
    fn line_isometry_on_missing_file_creates_then_removes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/hosts");
        let path = path.to_str().unwrap();

        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = line_glyph(path, "10.0.0.1 app");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert_eq!(fs::read_to_string(path).unwrap(), "10.0.0.1 app\n");

        rec.reverse(&outcome).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "");
    }

    #[test]
    fn line_reverse_only_removes_the_line_golem_added() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hosts");
        let path = path.to_str().unwrap();
        fs::write(path, "dup\ndup\n").unwrap();

        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = line_glyph(path, "dup");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(!outcome.changed);
        rec.reverse(&outcome).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "dup\ndup\n");
    }
}
