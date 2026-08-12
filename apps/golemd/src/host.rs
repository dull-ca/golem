//! The host command port. The apt and systemd reconcilers reach the host only
//! through [`CommandRunner`], so their logic is testable without touching the
//! real machine: [`SystemCommandRunner`] shells out for production, and the
//! `fake` module models an in-memory host for tests. (The file and lineInFile
//! reconcilers do real filesystem I/O directly — see `reconcilers.rs` — and are
//! tested against tempfiles instead.)

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;

use crate::progress::EventLevel;
use crate::reconciler::{EnactError, EnactResult};

/// The captured result of one command: its exit status and both output streams.
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn succeeded(&self) -> bool {
        self.status == 0
    }
}

/// The sink one streaming command forwards each output line to as it arrives:
/// `(level, line)` — stdout at `Info`, stderr at `Warn` (ADR 0033 §2). The
/// reconciler builds a closure that records each line into the progress ring
/// tagged `{unit_path, glyph_key, kind:"cmd"}`.
pub type CommandSink<'a> = dyn FnMut(EventLevel, &str) + 'a;

/// Run one host command and capture its output. The seam between the apt/systemd
/// reconcilers and the real machine.
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> EnactResult<CommandOutput>;

    /// Run a command, forwarding each stdout/stderr line to `sink` as it is
    /// produced while still returning the captured [`CommandOutput`] for the
    /// caller's success/stderr checks (ADR 0033 §2). The default delegates to
    /// [`CommandRunner::run`] and forwards nothing — an opt-in seam, so the fake
    /// runner and every existing call site are unchanged; only
    /// [`SystemCommandRunner`] overrides it to stream.
    fn run_streaming(
        &self,
        program: &str,
        args: &[&str],
        _sink: &mut CommandSink<'_>,
    ) -> EnactResult<CommandOutput> {
        self.run(program, args)
    }
}

/// The production runner: spawns the program via `std::process::Command`.
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, program: &str, args: &[&str]) -> EnactResult<CommandOutput> {
        let output = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| EnactError::Retryable(format!("spawn {program}: {e}")))?;
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn run_streaming(
        &self,
        program: &str,
        args: &[&str],
        sink: &mut CommandSink<'_>,
    ) -> EnactResult<CommandOutput> {
        let mut child = Command::new(program)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| EnactError::Retryable(format!("spawn {program}: {e}")))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (tx, rx) = mpsc::channel::<(EventLevel, String)>();
        let stdout_handle = stdout.map(|out| {
            let tx = tx.clone();
            thread::spawn(move || pump(out, EventLevel::Info, tx))
        });
        let stderr_handle = stderr.map(|err| {
            let tx = tx.clone();
            thread::spawn(move || pump(err, EventLevel::Warn, tx))
        });
        drop(tx);
        let mut stdout_lines: Vec<String> = Vec::new();
        let mut stderr_lines: Vec<String> = Vec::new();
        for (level, line) in rx {
            sink(level, &line);
            match level {
                EventLevel::Warn => stderr_lines.push(line),
                _ => stdout_lines.push(line),
            }
        }
        if let Some(h) = stdout_handle {
            let _ = h.join();
        }
        if let Some(h) = stderr_handle {
            let _ = h.join();
        }
        let status = child
            .wait()
            .map_err(|e| EnactError::Retryable(format!("wait {program}: {e}")))?;
        Ok(CommandOutput {
            status: status.code().unwrap_or(-1),
            stdout: join_lines(&stdout_lines),
            stderr: join_lines(&stderr_lines),
        })
    }
}

fn pump<R: std::io::Read>(stream: R, level: EventLevel, tx: mpsc::Sender<(EventLevel, String)>) {
    let reader = BufReader::new(stream);
    for line in reader.lines().map_while(Result::ok) {
        if tx.send((level, line)).is_err() {
            break;
        }
    }
}

fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        String::new()
    } else {
        let mut joined = lines.join("\n");
        joined.push('\n');
        joined
    }
}

/// An in-memory host that answers `dpkg-query`/`systemctl` and records
/// installs, enables, and starts, so the apt and systemd reconcilers can be
/// isometry-tested with no real host. Any command it does not model is a
/// `Fatal` error, which keeps a test honest about what it exercises.
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Mutex;

    /// Why a scripted `systemctl reset-failed` was refused. Both exit 1 and
    /// differ only in whether the latch survives: `AccessDenied` leaves the unit
    /// failed, so the start that follows is refused too; `UnitVanished` leaves it
    /// unfailed, since an unloaded unit has no failed state to hold.
    pub enum RefusedReset {
        AccessDenied,
        UnitVanished,
    }

    #[derive(Default)]
    pub struct FakeHost {
        installed: BTreeSet<String>,
        enabled: BTreeSet<String>,
        active: BTreeSet<String>,
        generated: BTreeSet<String>,
        failed: BTreeSet<String>,
        known: BTreeSet<String>,
        refused_resets: BTreeMap<String, RefusedReset>,
    }

    #[derive(Default)]
    pub struct FakeCommandRunner {
        host: Mutex<FakeHost>,
        log: Mutex<Vec<String>>,
    }

    impl FakeCommandRunner {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn with_installed(packages: &[&str]) -> Self {
            let runner = Self::new();
            {
                let mut host = runner.host.lock().unwrap();
                for p in packages {
                    host.installed.insert((*p).to_string());
                }
            }
            runner
        }

        pub fn with_service(unit: &str, enabled: bool, active: bool) -> Self {
            let runner = Self::new();
            {
                let mut host = runner.host.lock().unwrap();
                host.known.insert(unit.to_string());
                if enabled {
                    host.enabled.insert(unit.to_string());
                }
                if active {
                    host.active.insert(unit.to_string());
                }
            }
            runner
        }

        pub fn with_generated_service(unit: &str) -> Self {
            let runner = Self::new();
            {
                let mut host = runner.host.lock().unwrap();
                host.generated.insert(unit.to_string());
                host.enabled.insert(unit.to_string());
            }
            runner
        }

        /// Put `unit` in the failed state a real unit reaches by exhausting
        /// `StartLimitBurst`. Chaining rather than a constructor so it composes
        /// with both [`Self::with_service`] and [`Self::with_generated_service`] —
        /// a latched quadlet is the production shape — instead of doubling the
        /// constructor count.
        pub fn latched_failed(self, unit: &str) -> Self {
            {
                let mut host = self.host.lock().unwrap();
                host.failed.insert(unit.to_string());
                host.active.remove(unit);
            }
            self
        }

        /// Make `reset-failed` on `unit` exit 1 for the given reason. Chains onto
        /// [`Self::latched_failed`], since a refused reset is only observable on a
        /// unit that had a latch to clear.
        pub fn refusing_reset(self, unit: &str, refusal: RefusedReset) -> Self {
            {
                let mut host = self.host.lock().unwrap();
                host.refused_resets.insert(unit.to_string(), refusal);
            }
            self
        }

        pub fn is_installed(&self, package: &str) -> bool {
            self.host.lock().unwrap().installed.contains(package)
        }

        pub fn is_failed(&self, unit: &str) -> bool {
            self.host.lock().unwrap().failed.contains(unit)
        }

        pub fn is_enabled(&self, unit: &str) -> bool {
            self.host.lock().unwrap().enabled.contains(unit)
        }

        pub fn is_active(&self, unit: &str) -> bool {
            self.host.lock().unwrap().active.contains(unit)
        }

        pub fn log(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    /// A [`CommandRunner`] that **opts into** streaming (ADR 0033 §2): its
    /// `run_streaming` forwards a scripted list of stdout lines to the sink
    /// before delegating the state change to a wrapped [`FakeCommandRunner`], so
    /// a test can assert `cmd` lines flow without a real subprocess. The default
    /// `run` (non-streaming) forwards nothing, exactly like the production
    /// contract — only the streaming path emits.
    pub struct ScriptedStreamingRunner {
        inner: FakeCommandRunner,
        program: String,
        lines: Vec<String>,
    }

    impl ScriptedStreamingRunner {
        pub fn for_install(package: &str, lines: &[&str]) -> Self {
            let _ = package;
            Self {
                inner: FakeCommandRunner::new(),
                program: "apt-get".to_string(),
                lines: lines.iter().map(|l| l.to_string()).collect(),
            }
        }
    }

    impl CommandRunner for ScriptedStreamingRunner {
        fn run(&self, program: &str, args: &[&str]) -> EnactResult<CommandOutput> {
            self.inner.run(program, args)
        }

        fn run_streaming(
            &self,
            program: &str,
            args: &[&str],
            sink: &mut CommandSink<'_>,
        ) -> EnactResult<CommandOutput> {
            if program == self.program && args.first() == Some(&"install") {
                for line in &self.lines {
                    sink(EventLevel::Info, line);
                }
            }
            self.inner.run(program, args)
        }
    }

    /// The refusal real systemd returns for a unit latched in the failed state
    /// after exhausting `StartLimitBurst`. Every command that queues a start job
    /// — `enable`, `start`, `restart`, `reload-or-restart` — gets this until
    /// `reset-failed` clears the latch, which is what made a green reconcile hide
    /// three downed services (ADR 0057).
    fn start_refused_by_rate_limit(unit: &str) -> CommandOutput {
        CommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: format!("Job for {unit} failed because start request repeated too quickly."),
        }
    }

    fn reset_refused(unit: &str, reason: &str) -> CommandOutput {
        CommandOutput {
            status: 1,
            stdout: String::new(),
            stderr: format!("Failed to reset failed state of unit {unit}: {reason}."),
        }
    }

    impl CommandRunner for FakeCommandRunner {
        fn run(&self, program: &str, args: &[&str]) -> EnactResult<CommandOutput> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{program} {}", args.join(" ")));
            let mut host = self.host.lock().unwrap();
            match (program, args) {
                // Models the real dpkg-query batch trap this fake exists to
                // catch: exits 1 the instant any requested name is unknown,
                // while still printing a line for every name it does know.
                ("dpkg-query", _) if args.get(1).is_some_and(|f| f.contains("${Package}")) => {
                    let names = &args[2..];
                    let mut stdout = String::new();
                    let mut all_found = true;
                    for name in names {
                        if host.installed.contains(*name) {
                            stdout.push_str(&format!("{name} install ok installed\n"));
                        } else {
                            all_found = false;
                        }
                    }
                    Ok(CommandOutput {
                        status: if all_found { 0 } else { 1 },
                        stdout,
                        stderr: if all_found {
                            String::new()
                        } else {
                            "dpkg-query: no packages found matching one or more names".into()
                        },
                    })
                }
                ("dpkg-query", _) => {
                    let name = args.last().copied().unwrap_or_default();
                    if host.installed.contains(name) {
                        Ok(CommandOutput {
                            status: 0,
                            stdout: "install ok installed".into(),
                            stderr: String::new(),
                        })
                    } else {
                        Ok(CommandOutput {
                            status: 1,
                            stdout: String::new(),
                            stderr: "no packages found".into(),
                        })
                    }
                }
                // The apt apply refreshes the package list before install;
                // model the refresh as a no-op success so it does not trip the
                // unmodeled-command Fatal below.
                ("apt-get", ["update"]) => Ok(CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }),
                ("apt-get", _) if args.first() == Some(&"install") => {
                    for name in args.iter().skip(1).filter(|a| !a.starts_with('-')) {
                        host.installed.insert((*name).to_string());
                    }
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                ("apt-get", _) if args.first() == Some(&"remove") => {
                    let name = args.last().copied().unwrap_or_default();
                    host.installed.remove(name);
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                // The systemd apply reloads before enabling; model it as a
                // no-op so it does not trip the unmodeled-command Fatal below.
                ("systemctl", ["daemon-reload"]) => Ok(CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }),
                ("systemctl", _) if args.first() == Some(&"is-enabled") => {
                    let unit = args.last().copied().unwrap_or_default();
                    // `known` stands in for "systemd recognizes this unit at
                    // all": real `is-enabled` on a genuinely unknown unit
                    // prints "not-found" to stdout and exits 4 (measured on a
                    // Debian trixie guest), which is the signal
                    // `observe_systemd_verdict` keys on to report `Absent`. A
                    // known-but-disabled unit gets the ordinary "disabled\n" a
                    // real host would print.
                    let known = host.known.contains(unit)
                        || host.enabled.contains(unit)
                        || host.active.contains(unit)
                        || host.generated.contains(unit)
                        || host.failed.contains(unit);
                    if host.enabled.contains(unit) {
                        Ok(CommandOutput {
                            status: 0,
                            stdout: "enabled\n".into(),
                            stderr: String::new(),
                        })
                    } else if known {
                        Ok(CommandOutput {
                            status: 1,
                            stdout: "disabled\n".into(),
                            stderr: String::new(),
                        })
                    } else {
                        Ok(CommandOutput {
                            status: 4,
                            stdout: "not-found\n".into(),
                            stderr: String::new(),
                        })
                    }
                }
                ("systemctl", _) if args.first() == Some(&"is-active") => {
                    let unit = args.last().copied().unwrap_or_default();
                    if host.active.contains(unit) {
                        Ok(CommandOutput {
                            status: 0,
                            stdout: "active\n".into(),
                            stderr: String::new(),
                        })
                    } else {
                        Ok(CommandOutput {
                            status: 3,
                            stdout: "inactive\n".into(),
                            stderr: String::new(),
                        })
                    }
                }
                // `is-failed` exits 0 for a latched unit and non-zero otherwise,
                // the inverse convention of `is-enabled`/`is-active` reading as
                // "yes" on 0. A unit is never both failed and active, so
                // `latched_failed` drops it from `active` to keep the two answers
                // consistent.
                ("systemctl", _) if args.first() == Some(&"is-failed") => {
                    let unit = args.last().copied().unwrap_or_default();
                    if host.failed.contains(unit) {
                        Ok(CommandOutput {
                            status: 0,
                            stdout: "failed\n".into(),
                            stderr: String::new(),
                        })
                    } else {
                        Ok(CommandOutput {
                            status: 1,
                            stdout: "inactive\n".into(),
                            stderr: String::new(),
                        })
                    }
                }
                // No arm here starts, enables, or otherwise touches the unit — the
                // unit stays inactive whatever the outcome, so a test that sees it
                // active afterwards saw a real start.
                //
                // Whether the latch survives a refusal is the whole point of
                // [`RefusedReset`]. `AccessDenied` leaves it: nothing happened, and
                // the start that follows is refused too. `UnitVanished` drops it
                // even though the command exited 1, because a unit systemd has
                // unloaded has no failed state left to hold.
                ("systemctl", _) if args.first() == Some(&"reset-failed") => {
                    let unit = args.last().copied().unwrap_or_default();
                    match host.refused_resets.get(unit) {
                        Some(RefusedReset::AccessDenied) => {
                            Ok(reset_refused(unit, "Access denied"))
                        }
                        Some(RefusedReset::UnitVanished) => {
                            host.failed.remove(unit);
                            Ok(reset_refused(unit, &format!("Unit {unit} not loaded")))
                        }
                        None => {
                            host.failed.remove(unit);
                            Ok(CommandOutput {
                                status: 0,
                                stdout: String::new(),
                                stderr: String::new(),
                            })
                        }
                    }
                }
                ("systemctl", _) if args.first() == Some(&"enable") => {
                    let unit = args.last().copied().unwrap_or_default();
                    // A generated unit (a Podman quadlet) is enabled by its
                    // generator, so real systemd refuses enable with this exact
                    // text — the signal the apply's start fallback keys on.
                    if host.generated.contains(unit) {
                        return Ok(CommandOutput {
                            status: 1,
                            stdout: String::new(),
                            stderr: format!(
                                "Failed to enable unit: Unit {unit} is transient or generated."
                            ),
                        });
                    }
                    // `enable --now` is two steps, and a latched unit fails only
                    // the second: the `[Install]` symlinks are written, then the
                    // start job is refused. So the unit is inserted into `enabled`
                    // before the refusal returns, leaving it enabled but inactive —
                    // the state a real host is left in, and the state the apply's
                    // recorded inverse has to be correct about.
                    //
                    // The latch is checked after the generated-unit refusal above
                    // for the same fidelity: `enable` on a generated unit is
                    // rejected before any symlink or start job, so the latch never
                    // gets a say.
                    host.enabled.insert(unit.to_string());
                    if host.failed.contains(unit) {
                        return Ok(start_refused_by_rate_limit(unit));
                    }
                    host.active.insert(unit.to_string());
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                ("systemctl", _) if args.first() == Some(&"disable") => {
                    let unit = args.last().copied().unwrap_or_default();
                    host.enabled.remove(unit);
                    host.active.remove(unit);
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                ("systemctl", _) if args.first() == Some(&"stop") => {
                    let unit = args.last().copied().unwrap_or_default();
                    host.active.remove(unit);
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                ("systemctl", _) if args.first() == Some(&"start") => {
                    let unit = args.last().copied().unwrap_or_default();
                    if host.failed.contains(unit) {
                        return Ok(start_refused_by_rate_limit(unit));
                    }
                    host.active.insert(unit.to_string());
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                // The forcing verbs queue a start job, so a latched unit refuses
                // them exactly as it refuses `enable` and `start`. Clear the latch
                // first and the unit comes back active — that transition is what
                // the reset tests assert on.
                ("systemctl", _)
                    if args.first() == Some(&"restart")
                        || args.first() == Some(&"reload-or-restart") =>
                {
                    let unit = args.last().copied().unwrap_or_default();
                    if host.failed.contains(unit) {
                        return Ok(start_refused_by_rate_limit(unit));
                    }
                    host.active.insert(unit.to_string());
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                // The `try-` verbs act only on a *running* unit, and neither a
                // failed nor a merely inactive unit is running — so they exit 0
                // and change nothing at all, not even for a latched unit. That
                // silent success is the production bug: a green reconcile over a
                // service that never started. Modelled faithfully so a test that
                // takes this arm can only pass by leaving the unit down.
                ("systemctl", _)
                    if args.first() == Some(&"try-restart")
                        || args.first() == Some(&"try-reload-or-restart") =>
                {
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                // Best-effort forensics probes: model a failed unit so the
                // diagnose path has status and journal text to combine.
                ("systemctl", _) if args.first() == Some(&"status") => {
                    let unit = args.get(1).copied().unwrap_or_default();
                    Ok(CommandOutput {
                        status: 3,
                        stdout: format!("● {unit}\n     Active: failed (Result: exit-code)\n"),
                        stderr: String::new(),
                    })
                }
                ("journalctl", _) => {
                    let unit = args.get(1).copied().unwrap_or_default();
                    Ok(CommandOutput {
                        status: 0,
                        stdout: format!("{unit}: Failed with result 'exit-code'.\n"),
                        stderr: String::new(),
                    })
                }
                _ => Err(EnactError::Fatal(format!(
                    "unexpected command: {program} {}",
                    args.join(" ")
                ))),
            }
        }
    }
}
