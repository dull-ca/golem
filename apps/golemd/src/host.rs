//! The host command port. The apt and systemd reconcilers reach the host only
//! through [`CommandRunner`], so their logic is testable without touching the
//! real machine: [`SystemCommandRunner`] shells out for production, and the
//! `fake` module models an in-memory host for tests. (The file and lineInFile
//! reconcilers do real filesystem I/O directly — see `reconcilers.rs` — and are
//! tested against tempfiles instead.)

use std::process::Command;

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

/// Run one host command and capture its output. The seam between the apt/systemd
/// reconcilers and the real machine.
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> EnactResult<CommandOutput>;
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
