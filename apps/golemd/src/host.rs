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
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct FakeHost {
        installed: BTreeSet<String>,
        enabled: BTreeSet<String>,
        active: BTreeSet<String>,
        generated: BTreeSet<String>,
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

        pub fn is_installed(&self, package: &str) -> bool {
            self.host.lock().unwrap().installed.contains(package)
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

    impl CommandRunner for FakeCommandRunner {
        fn run(&self, program: &str, args: &[&str]) -> EnactResult<CommandOutput> {
            self.log
                .lock()
                .unwrap()
                .push(format!("{program} {}", args.join(" ")));
            let mut host = self.host.lock().unwrap();
            match (program, args) {
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
                    let name = args.last().copied().unwrap_or_default();
                    host.installed.insert(name.to_string());
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
                    if host.enabled.contains(unit) {
                        Ok(CommandOutput {
                            status: 0,
                            stdout: "enabled\n".into(),
                            stderr: String::new(),
                        })
                    } else {
                        Ok(CommandOutput {
                            status: 1,
                            stdout: "disabled\n".into(),
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
                    host.enabled.insert(unit.to_string());
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
                    host.active.insert(unit.to_string());
                    Ok(CommandOutput {
                        status: 0,
                        stdout: String::new(),
                        stderr: String::new(),
                    })
                }
                ("systemctl", _) if args.first() == Some(&"try-restart") => Ok(CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                }),
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
