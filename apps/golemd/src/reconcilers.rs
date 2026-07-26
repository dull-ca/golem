//! The real host reconcilers for the four glyph kinds (ADR 0015 §4). Each
//! `apply` first observes the host so re-applying a matching glyph is a no-op,
//! then captures the exact prior state as an [`Inverse`] so `reverse` restores
//! it — never removing a package, unit, filesystem entry, or line the host
//! already had. apt and systemd go through the [`CommandRunner`] port; the
//! `filesystem` and `lineInFile` glyphs do real filesystem I/O (atomic
//! temp-file-and-rename for file writes) and are tested against tempfiles.
//!
//! The `filesystem` glyph is one reconciler kind over three entry kinds (ADR
//! 0019): `apply` dispatches on the [`Entry`] sum to [`apply_file`],
//! [`apply_directory`], or [`apply_symlink`]. Directory and symlink creation are
//! governed by the same "reverse only what golem created" discipline as the rest
//! of the module — golem removes only empty directories it made (deepest-first,
//! stopping at any non-empty or pre-existing component) and refuses to clobber a
//! pre-existing entry rather than record an inverse it should not own.
//!
//! Two limits stand for now (ADR 0015):
//! - The `file` inverse holds the prior contents **inline** and reads them as a
//!   `String`, so a non-UTF-8 prior file is a `Fatal` error rather than a
//!   restorable inverse. Binary files and out-of-line blob storage are deferred.
//! - These adapters are exercised via the fake `CommandRunner` and tempfiles;
//!   the end-to-end run against a real Debian box (install → upgrade →
//!   decommission) is deferred to a later phase.
//!
//! The `systemdService` apply runs `daemon-reload` before `enable --now`: a
//! freshly written unit file — whether golem wrote it directly or a Podman
//! quadlet generated it — is invisible to systemd until a reload. Found running
//! golem on a real Debian box (ADR 0015 addendum).

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use nix::unistd::{chown, Gid, Group, Uid, User};
use scroll_format::{ContentId, Entry, Glyph, Perms};
use tracing::warn;

use crate::host::{CommandRunner, CommandSink, SystemCommandRunner};
use crate::journal::{GlyphOp, Inverse, Outcome};
use crate::reconciler::{EnactError, EnactResult, PrepareOutcome, Reconciler};

/// Enacts the four glyph kinds on a real host, driving apt and systemd through a
/// [`CommandRunner`] `R` (the `system()` constructor uses the real one; tests
/// inject the fake).
pub struct HostReconciler<R: CommandRunner> {
    runner: R,
    apt: std::sync::Mutex<()>,
}

impl HostReconciler<SystemCommandRunner> {
    pub fn system() -> Self {
        Self {
            runner: SystemCommandRunner,
            apt: std::sync::Mutex::new(()),
        }
    }
}

impl<R: CommandRunner> HostReconciler<R> {
    pub fn with_runner(runner: R) -> Self {
        Self {
            runner,
            apt: std::sync::Mutex::new(()),
        }
    }

    /// Install `names` in one `apt-get install` invocation, falling back to a
    /// per-package install if the batch fails to resolve (ADR 0034 §2). Returns
    /// `Ok(())`: a fallback package that still will not install is `Retryable`,
    /// so the round loop retries that glyph individually.
    fn batch_install(&self, names: &[String]) -> EnactResult<()> {
        let _guard = self.apt.lock().unwrap_or_else(|p| p.into_inner());
        let mut args: Vec<&str> = vec!["install", "-y"];
        for n in names {
            args.push(n.as_str());
        }
        let installed = self.runner.run("apt-get", &args)?;
        if installed.succeeded() {
            return Ok(());
        }
        for n in names {
            let one = self.runner.run("apt-get", &["install", "-y", n])?;
            if !one.succeeded() {
                return Err(EnactError::Retryable(format!(
                    "apt-get install {n}: {}",
                    one.stderr
                )));
            }
        }
        Ok(())
    }

    /// Install `name` if absent, capturing an [`Inverse::RemoveAptPackage`] so
    /// reverse removes only a package golem installed. `apt-get update` runs
    /// before the install: a fresh Debian cloud image ships with an empty
    /// package list, so `install` would fail to resolve the package without a
    /// refresh first.
    ///
    /// The refresh is per-glyph and idempotent — every apt install pays for one.
    /// A single refresh per reconcile would be cheaper, but this stateless
    /// per-glyph adapter has no reconcile-scoped hook to hang one update on
    /// without threading shared state through the port.
    fn apply_apt(
        &self,
        name: &str,
        cid: ContentId,
        glyph: &Glyph,
        sink: &mut CommandSink<'_>,
    ) -> EnactResult<Outcome> {
        if self.apt_installed(name)? {
            return Ok(outcome(glyph, cid, Inverse::Nothing, false));
        }
        let updated = self.runner.run_streaming("apt-get", &["update"], sink)?;
        if !updated.succeeded() {
            return Err(EnactError::Retryable(format!(
                "apt-get update: {}",
                updated.stderr
            )));
        }
        let installed = self
            .runner
            .run_streaming("apt-get", &["install", "-y", name], sink)?;
        if !installed.succeeded() {
            return Err(EnactError::Retryable(format!(
                "apt-get install {name}: {}",
                installed.stderr
            )));
        }
        Ok(outcome(
            glyph,
            cid,
            Inverse::RemoveAptPackage {
                name: name.to_string(),
            },
            true,
        ))
    }

    fn apt_installed(&self, name: &str) -> EnactResult<bool> {
        let query = self
            .runner
            .run("dpkg-query", &["-W", "-f=${Status}", name])?;
        Ok(query.succeeded() && query.stdout.contains("install ok installed"))
    }

    /// Enable and start `unit`, reloading first so a just-written unit file is
    /// visible to systemd. `daemon-reload` runs before `enable --now`; a failed
    /// reload is `Retryable`, same as a failed enable. The reverse path
    /// ([`Self::reverse_systemd`]) does not reload — reverse never writes a unit
    /// file, so the unit is already loaded, and deleting a unit golem wrote is
    /// the `file` glyph's own inverse.
    ///
    /// A generated unit refuses `enable`. A Podman quadlet is already enabled by
    /// its generator through an `[Install]` section, so `systemctl enable --now`
    /// rejects it as "transient or generated" rather than starting it. On that
    /// specific failure, fall back to `systemctl start` and record
    /// `started_only` so reverse *stops* the unit and never disables it — golem
    /// only started the unit, it never enabled it. Any other enable failure
    /// stays `Retryable`.
    ///
    /// The generated-unit test is a stderr substring today
    /// ([`is_generated_unit`]). A structural probe — `systemctl is-enabled`
    /// returning `generated` — is the more robust future signal, independent of
    /// systemd's error wording.
    fn apply_systemd(
        &self,
        unit: &str,
        cid: ContentId,
        glyph: &Glyph,
        sink: &mut CommandSink<'_>,
    ) -> EnactResult<Outcome> {
        let prior_enabled = self.systemd_enabled(unit)?;
        let prior_active = self.systemd_active(unit)?;
        if prior_enabled && prior_active {
            return Ok(outcome(glyph, cid, Inverse::Nothing, false));
        }
        let reloaded = self
            .runner
            .run_streaming("systemctl", &["daemon-reload"], sink)?;
        if !reloaded.succeeded() {
            return Err(EnactError::Retryable(format!(
                "systemctl daemon-reload: {}",
                reloaded.stderr
            )));
        }
        let enabled = self
            .runner
            .run_streaming("systemctl", &["enable", "--now", unit], sink)?;
        let started_only = if enabled.succeeded() {
            false
        } else if is_generated_unit(&enabled.stderr) {
            let started = self
                .runner
                .run_streaming("systemctl", &["start", unit], sink)?;
            if !started.succeeded() {
                return Err(EnactError::Retryable(format!(
                    "systemctl start {unit}: {}",
                    started.stderr
                )));
            }
            true
        } else {
            return Err(EnactError::Retryable(format!(
                "systemctl enable --now {unit}: {}",
                enabled.stderr
            )));
        };
        Ok(outcome(
            glyph,
            cid,
            Inverse::DisableSystemdService {
                unit: unit.to_string(),
                prior_enabled,
                prior_active,
                started_only,
            },
            true,
        ))
    }

    fn systemd_enabled(&self, unit: &str) -> EnactResult<bool> {
        Ok(self
            .runner
            .run("systemctl", &["is-enabled", unit])?
            .succeeded())
    }

    fn systemd_active(&self, unit: &str) -> EnactResult<bool> {
        Ok(self
            .runner
            .run("systemctl", &["is-active", unit])?
            .succeeded())
    }

    fn reverse_apt(&self, name: &str) -> EnactResult<()> {
        let removed = self.runner.run("apt-get", &["remove", "-y", name])?;
        if !removed.succeeded() {
            return Err(EnactError::Retryable(format!(
                "apt-get remove {name}: {}",
                removed.stderr
            )));
        }
        Ok(())
    }

    /// Restore the unit's prior state. A `started_only` unit — one golem could
    /// only start because `enable` refused it (a generated/quadlet unit) — is
    /// stopped, never disabled: golem never enabled it, so its enabled state is
    /// the generator's to own. Otherwise restore the recorded prior state:
    /// disable if golem enabled it, stop if golem started an inactive unit, else
    /// leave it.
    fn reverse_systemd(
        &self,
        unit: &str,
        prior_enabled: bool,
        prior_active: bool,
        started_only: bool,
    ) -> EnactResult<()> {
        if started_only {
            let stopped = self.runner.run("systemctl", &["stop", unit])?;
            if !stopped.succeeded() {
                return Err(EnactError::Retryable(format!(
                    "systemctl stop {unit}: {}",
                    stopped.stderr
                )));
            }
            return Ok(());
        }
        if !prior_enabled {
            let disabled = self.runner.run("systemctl", &["disable", "--now", unit])?;
            if !disabled.succeeded() {
                return Err(EnactError::Retryable(format!(
                    "systemctl disable --now {unit}: {}",
                    disabled.stderr
                )));
            }
            return Ok(());
        }
        if !prior_active {
            let stopped = self.runner.run("systemctl", &["stop", unit])?;
            if !stopped.succeeded() {
                return Err(EnactError::Retryable(format!(
                    "systemctl stop {unit}: {}",
                    stopped.stderr
                )));
            }
        }
        Ok(())
    }
}

impl<R: CommandRunner> HostReconciler<R> {
    fn try_restart(&self, unit: &str) -> EnactResult<()> {
        let reloaded = self.runner.run("systemctl", &["daemon-reload"])?;
        if !reloaded.succeeded() {
            return Err(EnactError::Retryable(format!(
                "systemctl daemon-reload: {}",
                reloaded.stderr
            )));
        }
        let restarted = self.runner.run("systemctl", &["try-restart", unit])?;
        if !restarted.succeeded() {
            return Err(EnactError::Retryable(format!(
                "systemctl try-restart {unit}: {}",
                restarted.stderr
            )));
        }
        Ok(())
    }

    fn diagnose_systemd(&self, unit: &str) -> Option<String> {
        let mut sections = Vec::new();
        if let Some(section) = self.probe(
            &format!("systemctl status {unit}"),
            "systemctl",
            &["status", unit, "--no-pager"],
        ) {
            sections.push(section);
        }
        if let Some(section) = self.probe(
            &format!("journalctl -u {unit}"),
            "journalctl",
            &["-u", unit, "--no-pager", "-n", "50"],
        ) {
            sections.push(section);
        }
        if sections.is_empty() {
            None
        } else {
            Some(sections.join("\n\n"))
        }
    }

    fn probe(&self, label: &str, program: &str, args: &[&str]) -> Option<String> {
        let output = self.runner.run(program, args).ok()?;
        let mut body = output.stdout;
        if !output.stderr.trim().is_empty() {
            if !body.is_empty() && !body.ends_with('\n') {
                body.push('\n');
            }
            body.push_str(&output.stderr);
        }
        if body.trim().is_empty() {
            return None;
        }
        Some(format!("=== {label} ===\n{}", body.trim_end()))
    }
}

impl<R: CommandRunner> Reconciler for HostReconciler<R> {
    fn apply(&self, glyph: &Glyph, cid: ContentId) -> EnactResult<Outcome> {
        let mut discard = |_level, _line: &str| {};
        self.apply_streaming(glyph, cid, &mut discard)
    }

    /// Dispatch on the glyph kind, routing the command-driven kinds (apt,
    /// systemd) through the streaming runner so their output reaches `sink` (ADR
    /// 0033 §2). The filesystem and `lineInFile` kinds run no command, so they
    /// never touch `sink` — command streaming cannot leak file contents.
    ///
    /// The systemd stream is only what `systemctl` itself prints. A unit's own
    /// process logs to the journal, not to systemctl's stdout, so a started
    /// service's output is invisible here — `diagnose` reads the journal after a
    /// failure, but the live tail carries only the enable/start commands.
    fn apply_streaming(
        &self,
        glyph: &Glyph,
        cid: ContentId,
        sink: &mut CommandSink<'_>,
    ) -> EnactResult<Outcome> {
        match glyph {
            Glyph::AptPackage { name } => self.apply_apt(name, cid, glyph, sink),
            Glyph::SystemdService { unit } => self.apply_systemd(unit, cid, glyph, sink),
            Glyph::Filesystem { path, entry } => match entry {
                Entry::File { contents, perms } => apply_file(path, contents, perms, cid, glyph),
                Entry::Directory { perms } => apply_directory(path, perms, cid, glyph),
                Entry::Symlink { target } => apply_symlink(path, target, cid, glyph),
            },
            Glyph::LineInFile { path, line } => apply_line_in_file(path, line, cid, glyph),
        }
    }

    fn prepare(&self, ops: &[GlyphOp]) -> EnactResult<PrepareOutcome> {
        let names = apt_install_names(ops);
        if names.is_empty() {
            return Ok(PrepareOutcome::default());
        }
        let absent_before: Vec<String> = names
            .iter()
            .filter(|n| !self.apt_installed(n).unwrap_or(false))
            .cloned()
            .collect();
        if let Err(e) = self.batch_install(&names) {
            warn!(
                error = %format!("{e:?}"),
                "batch install (and its per-glyph fallback) reported a failure; \
                 per-unit enact will classify the still-unresolved packages"
            );
        }
        let mut batch_installed = std::collections::HashSet::new();
        for name in absent_before {
            if self.apt_installed(&name).unwrap_or(false) {
                batch_installed.insert(name);
            }
        }
        Ok(PrepareOutcome { batch_installed })
    }

    fn reverse(&self, outcome: &Outcome) -> EnactResult<()> {
        match &outcome.inverse {
            Inverse::Nothing => Ok(()),
            Inverse::RemoveAptPackage { name } => self.reverse_apt(name),
            Inverse::DisableSystemdService {
                unit,
                prior_enabled,
                prior_active,
                started_only,
            } => self.reverse_systemd(unit, *prior_enabled, *prior_active, *started_only),
            Inverse::RestoreFile {
                path,
                contents,
                perms,
            } => restore_file(path, contents, perms),
            Inverse::DeleteFile { path } => delete_file(path),
            Inverse::RemoveDirectory { path, created } => remove_directory(path, created),
            Inverse::RestoreDirMeta { path, prior_perms } => apply_perms(path, prior_perms),
            Inverse::RemoveSymlink { path } => remove_symlink(path),
            Inverse::RemoveLineInFile { path, line } => remove_line_in_file(path, line),
        }
    }

    fn restart_unit(&self, unit: &str) -> EnactResult<()> {
        self.try_restart(unit)
    }

    fn diagnose(&self, glyph: &Glyph) -> Option<String> {
        match glyph {
            Glyph::SystemdService { unit } => self.diagnose_systemd(unit),
            _ => None,
        }
    }
}

/// Whether a failed `enable` was refused because the unit is generated —
/// detected by systemd's "transient or generated" error text. A substring match
/// on stderr, not a structural check; see [`HostReconciler::apply_systemd`] for
/// why and the sturdier signal to move to.
fn is_generated_unit(stderr: &str) -> bool {
    stderr.contains("transient or generated")
}

fn apt_install_names(ops: &[GlyphOp]) -> Vec<String> {
    let mut names = Vec::new();
    for op in ops {
        if let GlyphOp::Install {
            glyph: Glyph::AptPackage { name },
            ..
        } = op
        {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
    }
    names
}

fn outcome(glyph: &Glyph, cid: ContentId, inverse: Inverse, changed: bool) -> Outcome {
    Outcome {
        op: GlyphOp::Install {
            cid,
            glyph: glyph.clone(),
        },
        cid,
        inverse,
        changed,
    }
}

fn apply_file(
    path: &str,
    contents: &str,
    perms: &Perms,
    cid: ContentId,
    glyph: &Glyph,
) -> EnactResult<Outcome> {
    let prior = read_file(path)?;
    if let Some((prior_contents, prior_perms)) = &prior {
        if prior_contents == contents && perms_match(prior_perms, perms)? {
            return Ok(outcome(glyph, cid, Inverse::Nothing, false));
        }
    }
    write_file_atomic(path, contents, perms)?;
    let inverse = match prior {
        Some((prior_contents, prior_perms)) => Inverse::RestoreFile {
            path: path.to_string(),
            contents: prior_contents,
            perms: prior_perms,
        },
        None => Inverse::DeleteFile {
            path: path.to_string(),
        },
    };
    Ok(outcome(glyph, cid, inverse, true))
}

/// Ensure a directory at `path` with `perms`, dispatched from the `Directory`
/// entry arm. Three cases (ADR 0019 §3), each recording an inverse that reverses
/// exactly what this apply did and no more:
///
/// - **Already a directory** — restore `perms` if they differ
///   ([`Inverse::RestoreDirMeta`] holding the prior perms, so reverse chmod/chowns
///   back and never removes the directory); no-op if they already match.
/// - **Absent** — `create_dir_all` via [`create_missing_components`], then set
///   perms/ownership. The inverse ([`Inverse::RemoveDirectory`]) carries the
///   exact components golem created so reverse removes only those (see
///   [`remove_directory`]).
/// - **A non-directory already at `path`** — refuse with `Fatal` rather than
///   clobber a pre-existing file/symlink golem did not create (the ADR 0015
///   "never touch state it did not record creating" rule).
fn apply_directory(
    path: &str,
    perms: &Perms,
    cid: ContentId,
    glyph: &Glyph,
) -> EnactResult<Outcome> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => {
            let prior_perms = observe_perms(path)?;
            if perms_match(&prior_perms, perms)? {
                return Ok(outcome(glyph, cid, Inverse::Nothing, false));
            }
            apply_perms(path, perms)?;
            Ok(outcome(
                glyph,
                cid,
                Inverse::RestoreDirMeta {
                    path: path.to_string(),
                    prior_perms,
                },
                true,
            ))
        }
        Ok(_) => Err(EnactError::Fatal(format!(
            "refuse to replace pre-existing non-directory at {path} with a directory"
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let created = create_missing_components(path)?;
            apply_perms(path, perms)?;
            Ok(outcome(
                glyph,
                cid,
                Inverse::RemoveDirectory {
                    path: path.to_string(),
                    created,
                },
                true,
            ))
        }
        Err(e) => Err(EnactError::Retryable(format!("stat {path}: {e}"))),
    }
}

/// Ensure a symlink `path` -> `target`, dispatched from the `Symlink` entry arm.
/// The arm carries no `Perms`, so there is no mode to set — a symlink's own mode
/// is not honoured on Linux (ADR 0019 §3). Cases:
///
/// - **Already a symlink to `target`** — no-op. A symlink pointing *elsewhere* is
///   refused with `Fatal`: golem did not create it, so repointing it would clobber
///   pre-existing state.
/// - **Absent** — `mkdir -p` the parent, create the symlink, record
///   [`Inverse::RemoveSymlink`] so reverse `unlink`s only the link golem made.
/// - **A non-symlink already at `path`** — refuse with `Fatal` rather than replace
///   a pre-existing entry (the ADR 0015 discipline; this is the one genuinely new
///   hazard the symlink arm introduces).
fn apply_symlink(path: &str, target: &str, cid: ContentId, glyph: &Glyph) -> EnactResult<Outcome> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let current = fs::read_link(path)
                .map_err(|e| EnactError::Retryable(format!("readlink {path}: {e}")))?;
            if current == Path::new(target) {
                return Ok(outcome(glyph, cid, Inverse::Nothing, false));
            }
            Err(EnactError::Fatal(format!(
                "refuse to repoint pre-existing symlink at {path} (points to {})",
                current.display()
            )))
        }
        Ok(_) => Err(EnactError::Fatal(format!(
            "refuse to replace pre-existing entry at {path} with a symlink"
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let parent = Path::new(path).parent().unwrap_or_else(|| Path::new("."));
            fs::create_dir_all(parent)
                .map_err(|e| EnactError::Retryable(format!("mkdir {}: {e}", parent.display())))?;
            std::os::unix::fs::symlink(target, path)
                .map_err(|e| EnactError::Retryable(format!("symlink {path} -> {target}: {e}")))?;
            Ok(outcome(
                glyph,
                cid,
                Inverse::RemoveSymlink {
                    path: path.to_string(),
                },
                true,
            ))
        }
        Err(e) => Err(EnactError::Retryable(format!("stat {path}: {e}"))),
    }
}

/// The prior `(contents, perms)` of a file, or `None` if it is absent — the
/// state captured for the `file` inverse. Reads contents as a `String`, so a
/// non-UTF-8 prior file is a `Fatal` error (the inline-inverse limit noted in
/// the module doc).
fn read_file(path: &str) -> EnactResult<Option<(String, Perms)>> {
    match fs::read(path) {
        Ok(bytes) => {
            let contents = String::from_utf8(bytes)
                .map_err(|e| EnactError::Fatal(format!("read {path}: not utf-8: {e}")))?;
            Ok(Some((contents, observe_perms(path)?)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(EnactError::Retryable(format!("read {path}: {e}"))),
    }
}

/// The permission bits and ownership currently at `path`, captured for a
/// restoring inverse. Ownership is recorded as resolved names when the uid/gid
/// resolves, so reverse re-resolves them on the host the same way apply does.
fn observe_perms(path: &str) -> EnactResult<Perms> {
    use std::os::unix::fs::MetadataExt;
    let meta =
        fs::metadata(path).map_err(|e| EnactError::Retryable(format!("stat {path}: {e}")))?;
    Ok(Perms {
        mode: (meta.permissions().mode() & 0o7777) as u16,
        owner: User::from_uid(Uid::from_raw(meta.uid()))
            .ok()
            .flatten()
            .map(|u| u.name),
        group: Group::from_gid(Gid::from_raw(meta.gid()))
            .ok()
            .flatten()
            .map(|g| g.name),
    })
}

/// Whether the host's current `prior` perms already realize the `desired` ones:
/// same mode, and for each of owner/group the desired name (when set) resolves
/// to the same id the prior name resolves to. An unset desired owner/group is
/// "leave as-is", so it always matches.
fn perms_match(prior: &Perms, desired: &Perms) -> EnactResult<bool> {
    if prior.mode != desired.mode {
        return Ok(false);
    }
    if let Some(name) = &desired.owner {
        if resolve_uid(name)?
            != prior
                .owner
                .as_deref()
                .map(resolve_uid)
                .transpose()?
                .flatten()
        {
            return Ok(false);
        }
    }
    if let Some(name) = &desired.group {
        if resolve_gid(name)?
            != prior
                .group
                .as_deref()
                .map(resolve_gid)
                .transpose()?
                .flatten()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Bring `path`'s ownership and mode to `perms`: resolve each set owner/group
/// name to a uid/gid and `chown` (an unset name is "leave as-is" — no `chown` for
/// that axis), then `chmod` to `mode`. An owner/group name that does not resolve
/// on the host is a `Fatal`. Invoked only from the arms that carry a `Perms`
/// (`File`, `Directory`) — the type forbids ever applying a mode to a symlink.
/// Also serves as the reverse of [`Inverse::RestoreDirMeta`], restoring a
/// directory's prior perms.
fn apply_perms(path: &str, perms: &Perms) -> EnactResult<()> {
    let uid = match &perms.owner {
        Some(name) => {
            Some(Uid::from_raw(resolve_uid(name)?.ok_or_else(|| {
                EnactError::Fatal(format!("unknown owner `{name}`"))
            })?))
        }
        None => None,
    };
    let gid = match &perms.group {
        Some(name) => {
            Some(Gid::from_raw(resolve_gid(name)?.ok_or_else(|| {
                EnactError::Fatal(format!("unknown group `{name}`"))
            })?))
        }
        None => None,
    };
    if uid.is_some() || gid.is_some() {
        chown(Path::new(path), uid, gid)
            .map_err(|e| EnactError::Retryable(format!("chown {path}: {e}")))?;
    }
    fs::set_permissions(path, fs::Permissions::from_mode(perms.mode as u32))
        .map_err(|e| EnactError::Retryable(format!("chmod {path}: {e}")))
}

fn resolve_uid(name: &str) -> EnactResult<Option<u32>> {
    Ok(User::from_name(name)
        .map_err(|e| EnactError::Retryable(format!("resolve owner `{name}`: {e}")))?
        .map(|u| u.uid.as_raw()))
}

fn resolve_gid(name: &str) -> EnactResult<Option<u32>> {
    Ok(Group::from_name(name)
        .map_err(|e| EnactError::Retryable(format!("resolve group `{name}`: {e}")))?
        .map(|g| g.gid.as_raw()))
}

/// The path's missing ancestor components, created deepest-first: walk up to the
/// first existing ancestor, then `mkdir` each missing component top-down and
/// return them deepest-first — the exact list `reverse` `rmdir`s to undo only
/// what golem created.
fn create_missing_components(path: &str) -> EnactResult<Vec<String>> {
    let target = Path::new(path);
    let mut missing: Vec<PathBuf> = Vec::new();
    let mut cursor = Some(target);
    while let Some(component) = cursor {
        if component.as_os_str().is_empty() || component.exists() {
            break;
        }
        missing.push(component.to_path_buf());
        cursor = component.parent();
    }
    let mut created = Vec::with_capacity(missing.len());
    for component in missing.iter().rev() {
        fs::create_dir(component)
            .map_err(|e| EnactError::Retryable(format!("mkdir {}: {e}", component.display())))?;
    }
    for component in missing {
        created.push(component.to_string_lossy().into_owned());
    }
    Ok(created)
}

/// Write `contents` to `path` at `perms` atomically: fill a temp file in the
/// target directory, set its permissions and ownership, then rename it over the
/// target, so a reader never sees a half-written file.
fn write_file_atomic(path: &str, contents: &str, perms: &Perms) -> EnactResult<()> {
    let target = Path::new(path);
    let dir = target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)
        .map_err(|e| EnactError::Retryable(format!("mkdir {}: {e}", dir.display())))?;
    let mut temp = tempfile::NamedTempFile::new_in(dir)
        .map_err(|e| EnactError::Retryable(format!("temp in {}: {e}", dir.display())))?;
    temp.write_all(contents.as_bytes())
        .map_err(|e| EnactError::Retryable(format!("write temp for {path}: {e}")))?;
    temp.flush()
        .map_err(|e| EnactError::Retryable(format!("flush temp for {path}: {e}")))?;
    let temp_path = temp
        .path()
        .to_str()
        .ok_or_else(|| EnactError::Fatal(format!("non-utf8 temp path for {path}")))?
        .to_string();
    apply_perms(&temp_path, perms)?;
    temp.persist(target)
        .map_err(|e| EnactError::Retryable(format!("persist {path}: {e}")))?;
    Ok(())
}

fn restore_file(path: &str, contents: &str, perms: &Perms) -> EnactResult<()> {
    write_file_atomic(path, contents, perms)
}

fn delete_file(path: &str) -> EnactResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(EnactError::Retryable(format!("remove {path}: {e}"))),
    }
}

/// Remove only the empty directory components golem created, deepest-first,
/// stopping at the first that is non-empty (a later glyph or a container may
/// have populated it). Never `rm -rf`.
fn remove_directory(path: &str, created: &[String]) -> EnactResult<()> {
    let _ = path;
    for component in created {
        match fs::remove_dir(component) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) if is_not_empty(&e) => break,
            Err(e) => return Err(EnactError::Retryable(format!("rmdir {component}: {e}"))),
        }
    }
    Ok(())
}

fn is_not_empty(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(nix::libc::ENOTEMPTY) || e.raw_os_error() == Some(nix::libc::EEXIST)
}

fn remove_symlink(path: &str) -> EnactResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(EnactError::Retryable(format!("unlink {path}: {e}"))),
    }
}

fn apply_line_in_file(
    path: &str,
    line: &str,
    cid: ContentId,
    glyph: &Glyph,
) -> EnactResult<Outcome> {
    if file_has_line(path, line)? {
        return Ok(outcome(glyph, cid, Inverse::Nothing, false));
    }
    append_line(path, line)?;
    Ok(outcome(
        glyph,
        cid,
        Inverse::RemoveLineInFile {
            path: path.to_string(),
            line: line.to_string(),
        },
        true,
    ))
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
    fs::create_dir_all(dir)
        .map_err(|e| EnactError::Retryable(format!("mkdir {}: {e}", dir.display())))?;
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
    use crate::host::fake::{FakeCommandRunner, ScriptedStreamingRunner};
    use crate::progress::EventLevel;
    use crate::reconcile::glyph_content_id;

    #[test]
    fn apt_apply_streams_scripted_command_lines_to_the_sink() {
        let runner = ScriptedStreamingRunner::for_install(
            "podman",
            &["Unpacking podman (4.3.1) ...", "Setting up podman ..."],
        );
        let rec = HostReconciler::with_runner(runner);
        let glyph = apt("podman");
        let cid = glyph_content_id(&glyph);

        let mut seen: Vec<(EventLevel, String)> = Vec::new();
        let mut sink = |level: EventLevel, line: &str| seen.push((level, line.to_string()));
        let outcome = rec.apply_streaming(&glyph, cid, &mut sink).unwrap();

        assert!(outcome.changed);
        let messages: Vec<&str> = seen.iter().map(|(_, m)| m.as_str()).collect();
        assert!(
            messages.contains(&"Unpacking podman (4.3.1) ..."),
            "expected the install output to stream, saw {messages:?}"
        );
        assert!(messages.contains(&"Setting up podman ..."));
        assert!(
            seen.iter().all(|(lvl, _)| matches!(lvl, EventLevel::Info)),
            "scripted stdout lines stream at Info"
        );
    }

    #[test]
    fn filesystem_apply_streaming_touches_no_sink() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.conf");
        let path = path.to_str().unwrap();
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = file_glyph(path, "desired\n", 0o644);
        let cid = glyph_content_id(&glyph);

        let mut count = 0usize;
        let mut sink = |_lvl: EventLevel, _line: &str| count += 1;
        rec.apply_streaming(&glyph, cid, &mut sink).unwrap();
        assert_eq!(
            count, 0,
            "a file reconciler runs no command, so it streams nothing"
        );
    }

    fn apt(name: &str) -> Glyph {
        Glyph::AptPackage { name: name.into() }
    }

    fn systemd(unit: &str) -> Glyph {
        Glyph::SystemdService { unit: unit.into() }
    }

    fn perms(mode: u16) -> Perms {
        Perms {
            mode,
            owner: None,
            group: None,
        }
    }

    fn file_glyph(path: &str, contents: &str, mode: u16) -> Glyph {
        Glyph::Filesystem {
            path: path.into(),
            entry: Entry::File {
                contents: contents.into(),
                perms: perms(mode),
            },
        }
    }

    fn directory_glyph(path: &str, mode: u16) -> Glyph {
        Glyph::Filesystem {
            path: path.into(),
            entry: Entry::Directory { perms: perms(mode) },
        }
    }

    fn symlink_glyph(path: &str, target: &str) -> Glyph {
        Glyph::Filesystem {
            path: path.into(),
            entry: Entry::Symlink {
                target: target.into(),
            },
        }
    }

    fn line_glyph(path: &str, line: &str) -> Glyph {
        Glyph::LineInFile {
            path: path.into(),
            line: line.into(),
        }
    }

    #[test]
    fn diagnose_systemd_combines_status_and_journal_into_a_labeled_string() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let details = rec.diagnose(&systemd("fishnet.service")).unwrap();
        assert!(details.contains("=== systemctl status fishnet.service ==="));
        assert!(details.contains("Active: failed"));
        assert!(details.contains("=== journalctl -u fishnet.service ==="));
        assert!(details.contains("fishnet.service: Failed"));
    }

    #[test]
    fn diagnose_is_none_for_non_service_glyphs() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        assert!(rec.diagnose(&apt("podman")).is_none());
        assert!(rec.diagnose(&file_glyph("/etc/x", "y", 0o644)).is_none());
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
    fn apt_updates_package_list_before_installing() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = apt("nginx");
        let cid = glyph_content_id(&glyph);

        rec.apply(&glyph, cid).unwrap();

        let log = runner_of(&rec).log();
        let update = log.iter().position(|c| c == "apt-get update");
        let install = log.iter().position(|c| c == "apt-get install -y nginx");
        assert!(
            update.is_some(),
            "expected an apt-get update, log was {log:?}"
        );
        assert!(
            install.is_some(),
            "expected an apt-get install, log was {log:?}"
        );
        assert!(
            update < install,
            "apt-get update must precede install, log was {log:?}"
        );
    }

    #[test]
    fn apt_reapply_same_cid_is_unchanged() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = apt("nginx");
        let cid = glyph_content_id(&glyph);
        assert!(rec.apply(&glyph, cid).unwrap().changed);
        assert!(!rec.apply(&glyph, cid).unwrap().changed);
    }

    fn install_op(glyph: &Glyph) -> GlyphOp {
        GlyphOp::Install {
            cid: glyph_content_id(glyph),
            glyph: glyph.clone(),
        }
    }

    #[test]
    fn prepare_batches_all_apt_installs_into_one_invocation() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let ops = vec![install_op(&apt("podman")), install_op(&apt("htop"))];
        rec.prepare(&ops).unwrap();
        let log = runner_of(&rec).log();
        let batched = log.iter().any(|c| c == "apt-get install -y podman htop");
        assert!(batched, "expected one batched install, log was {log:?}");
        let per_glyph = log
            .iter()
            .filter(|c| c.as_str() == "apt-get install -y podman")
            .count();
        assert_eq!(per_glyph, 0, "no per-glyph install when the batch succeeds");
        assert!(runner_of(&rec).is_installed("podman") && runner_of(&rec).is_installed("htop"));
    }

    struct FlakyBatchRunner {
        inner: FakeCommandRunner,
    }
    impl FlakyBatchRunner {
        fn new() -> Self {
            Self {
                inner: FakeCommandRunner::new(),
            }
        }
        fn inner_installed(&self, p: &str) -> bool {
            self.inner.is_installed(p)
        }
    }
    impl CommandRunner for FlakyBatchRunner {
        fn run(&self, program: &str, args: &[&str]) -> EnactResult<crate::host::CommandOutput> {
            if program == "apt-get" && args.first() == Some(&"install") {
                let pkgs: Vec<&str> = args
                    .iter()
                    .skip(1)
                    .filter(|a| !a.starts_with('-'))
                    .copied()
                    .collect();
                if pkgs.len() > 1 {
                    return Ok(crate::host::CommandOutput {
                        status: 100,
                        stdout: String::new(),
                        stderr: "batch unresolved".into(),
                    });
                }
                if pkgs == ["nope"] {
                    return Ok(crate::host::CommandOutput {
                        status: 100,
                        stdout: String::new(),
                        stderr: "no such package nope".into(),
                    });
                }
            }
            self.inner.run(program, args)
        }
    }

    #[test]
    fn a_failed_batch_falls_back_to_per_glyph_installs() {
        let rec = HostReconciler::with_runner(FlakyBatchRunner::new());
        let ok = rec.prepare(&[install_op(&apt("podman")), install_op(&apt("htop"))]);
        assert!(
            ok.is_ok(),
            "two good packages install per-glyph after the batch fails"
        );

        let partial = rec.prepare(&[install_op(&apt("curl")), install_op(&apt("nope"))]);
        let outcome = partial.expect(
            "prepare stays Ok even when the fallback partially fails; the verify loop's dpkg \
             truth is the receipt, and per-unit enact classifies whatever is still unresolved",
        );
        assert!(
            outcome.batch_installed.contains("curl"),
            "the good sibling's dpkg-verified install is still reported, got {:?}",
            outcome.batch_installed
        );
        assert!(
            !outcome.batch_installed.contains("nope"),
            "the still-unresolved package is excluded, got {:?}",
            outcome.batch_installed
        );
        assert!(
            runner_of(&rec).inner_installed("curl"),
            "the good sibling still installed via fallback"
        );
        assert!(
            !runner_of(&rec).inner_installed("nope"),
            "the bad package never installed"
        );
    }

    #[test]
    fn systemd_daemon_reloads_before_enable() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::with_service("app", false, false));
        let glyph = systemd("app");
        let cid = glyph_content_id(&glyph);

        rec.apply(&glyph, cid).unwrap();

        let log = runner_of(&rec).log();
        let reload = log.iter().position(|c| c == "systemctl daemon-reload");
        let enable = log.iter().position(|c| c == "systemctl enable --now app");
        assert!(
            reload.is_some(),
            "expected a daemon-reload, log was {log:?}"
        );
        assert!(
            enable.is_some(),
            "expected an enable --now, log was {log:?}"
        );
        assert!(
            reload < enable,
            "daemon-reload must precede enable, log was {log:?}"
        );
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

    #[test]
    fn systemd_generated_unit_falls_back_to_start() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::with_generated_service("app"));
        let glyph = systemd("app");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert!(runner_of(&rec).is_active("app"));

        let log = runner_of(&rec).log();
        assert!(
            log.iter().any(|c| c == "systemctl enable --now app"),
            "expected an enable attempt, log was {log:?}"
        );
        assert!(
            log.iter().any(|c| c == "systemctl start app"),
            "expected a start fallback after enable was refused, log was {log:?}"
        );
        assert_eq!(
            outcome.inverse,
            Inverse::DisableSystemdService {
                unit: "app".into(),
                prior_enabled: true,
                prior_active: false,
                started_only: true,
            }
        );
    }

    #[test]
    fn systemd_generated_unit_reverse_stops_never_disables() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::with_generated_service("app"));
        let glyph = systemd("app");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(runner_of(&rec).is_active("app"));

        rec.reverse(&outcome).unwrap();
        assert!(!runner_of(&rec).is_active("app"));
        assert!(runner_of(&rec).is_enabled("app"));

        let log = runner_of(&rec).log();
        assert!(
            log.iter().any(|c| c == "systemctl stop app"),
            "reverse must stop the started-only unit, log was {log:?}"
        );
        assert!(
            !log.iter().any(|c| c.starts_with("systemctl disable")),
            "reverse must never disable a unit golem only started, log was {log:?}"
        );
    }

    #[test]
    fn systemd_plain_unit_records_enabled_not_started_only() {
        let rec = HostReconciler::with_runner(FakeCommandRunner::with_service("app", false, false));
        let glyph = systemd("app");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert_eq!(
            outcome.inverse,
            Inverse::DisableSystemdService {
                unit: "app".into(),
                prior_enabled: false,
                prior_active: false,
                started_only: false,
            }
        );
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
        let glyph = file_glyph(path, "desired\n", 0o644);
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
        let glyph = file_glyph(path, "desired\n", 0o644);
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
        let glyph = file_glyph(path, "desired\n", 0o644);
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
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "127.0.0.1 localhost\n10.0.0.1 app\n"
        );

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

    #[test]
    fn directory_isometry_when_absent_removes_only_created_components_on_reverse() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("srv");
        fs::create_dir(&base).unwrap();
        let target = base.join("registry/data");
        let path = target.to_str().unwrap();
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = directory_glyph(path, 0o755);
        let cid = glyph_content_id(&glyph);

        assert!(!target.exists());
        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert!(target.is_dir());
        assert_eq!(mode_of(path), 0o755);

        rec.reverse(&outcome).unwrap();
        assert!(!target.exists());
        assert!(!base.join("registry").exists());
        assert!(base.exists());
    }

    #[test]
    fn directory_reverse_stops_at_a_non_empty_component() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a/b/c");
        let path = target.to_str().unwrap();
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = directory_glyph(path, 0o755);
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(target.is_dir());
        fs::write(dir.path().join("a/b/keeper"), "x").unwrap();

        rec.reverse(&outcome).unwrap();
        assert!(!target.exists());
        assert!(dir.path().join("a/b").is_dir());
        assert!(dir.path().join("a").is_dir());
    }

    #[test]
    fn directory_reapply_same_mode_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data");
        let path = path.to_str().unwrap();
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = directory_glyph(path, 0o755);
        let cid = glyph_content_id(&glyph);

        assert!(rec.apply(&glyph, cid).unwrap().changed);
        assert!(!rec.apply(&glyph, cid).unwrap().changed);
    }

    #[test]
    fn directory_that_pre_existed_restores_prior_mode_and_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("data");
        let path = target.to_str().unwrap();
        fs::create_dir(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();

        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = directory_glyph(path, 0o755);
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert_eq!(mode_of(path), 0o755);

        rec.reverse(&outcome).unwrap();
        assert!(target.is_dir());
        assert_eq!(mode_of(path), 0o700);
    }

    #[test]
    fn directory_refuses_to_replace_a_pre_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("thing");
        let path = target.to_str().unwrap();
        fs::write(&target, "i am a file").unwrap();

        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = directory_glyph(path, 0o755);
        let cid = glyph_content_id(&glyph);

        match rec.apply(&glyph, cid) {
            Err(EnactError::Fatal(_)) => {}
            other => panic!("expected a Fatal refusal, got {other:?}"),
        }
        assert!(target.is_file());
    }

    #[test]
    fn symlink_isometry_when_absent_removes_on_reverse() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("enabled/site");
        let path = link.to_str().unwrap();
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = symlink_glyph(path, "/etc/available/site");
        let cid = glyph_content_id(&glyph);

        let outcome = rec.apply(&glyph, cid).unwrap();
        assert!(outcome.changed);
        assert_eq!(
            fs::read_link(path).unwrap(),
            Path::new("/etc/available/site")
        );

        rec.reverse(&outcome).unwrap();
        assert!(fs::symlink_metadata(path).is_err());
    }

    #[test]
    fn symlink_reapply_same_target_is_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("site");
        let path = link.to_str().unwrap();
        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = symlink_glyph(path, "/etc/available/site");
        let cid = glyph_content_id(&glyph);

        assert!(rec.apply(&glyph, cid).unwrap().changed);
        let second = rec.apply(&glyph, cid).unwrap();
        assert!(!second.changed);
        assert_eq!(second.inverse, Inverse::Nothing);
    }

    #[test]
    fn symlink_refuses_to_replace_a_pre_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("site");
        let path = target.to_str().unwrap();
        fs::write(&target, "real file").unwrap();

        let rec = HostReconciler::with_runner(FakeCommandRunner::new());
        let glyph = symlink_glyph(path, "/etc/available/site");
        let cid = glyph_content_id(&glyph);

        match rec.apply(&glyph, cid) {
            Err(EnactError::Fatal(_)) => {}
            other => panic!("expected a Fatal refusal, got {other:?}"),
        }
        assert!(target.is_file());
    }
}
