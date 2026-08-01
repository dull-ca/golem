//! A local port forwarded to a daemon's loopback port over ssh (ADR 0042).
//!
//! golemd binds `127.0.0.1` on a deployed host, so the only route to it is the
//! one every production box already has: an ssh session. [`Tunnel::open`]
//! spawns `ssh -N -L 127.0.0.1:<free>:127.0.0.1:<remote> <destination>` and
//! hands back the local port; [`crate::conn::Conn`] then speaks plain HTTP to
//! it. The ssh session is the encryption and the host authentication — golem
//! carries no TLS. An operator's own `ssh_config` still applies, so a
//! ControlMaster already up makes the forward cost milliseconds and golemctl
//! needs to know nothing about it.
//!
//! **Lifecycle.** `free_loopback_port` binds port 0, reads what the kernel
//! chose, and closes it — the port is then merely *likely* free, and the window
//! between closing and ssh binding is a real race. `ExitOnForwardFailure=yes`
//! closes the dangerous half of it: if something took the port first, ssh exits
//! instead of running on with no forward, and a request would otherwise be
//! addressed to a stranger holding that port. `await_forward` waits for the
//! port to answer *while ssh is still alive* — a port that answers after ssh
//! exited is that stranger, and is reported as one rather than used.
//! [`Drop`] kills the child and reaps it, so the forward never outlives the
//! verb even when the verb exits nonzero.
//!
//! **ssh's stderr** is piped and drained by a thread (`collect`), so its
//! diagnosis (`No route to host`, a key refusal) can be quoted in the failure.
//! Draining is not optional: an ssh that inherits golemctl's stderr can leave a
//! grandchild holding the pipe open, and reading to EOF on the main path would
//! hang the verb instead of failing it. The thread reads to EOF and publishes
//! once; `stderr_tail` waits a short grace for that and gives up rather than
//! block an error report.
//!
//! `GOLEMCTL_SSH` replaces the `ssh` binary — the seam the tests drive a fake
//! forwarder through, and an escape hatch for a non-default ssh.

use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

pub const SSH_BIN_ENV: &str = "GOLEMCTL_SSH";
pub const EXIT_ON_FORWARD_FAILURE: &str = "ExitOnForwardFailure=yes";
pub const DEFAULT_SSH_BIN: &str = "ssh";
pub const CONNECT_INTERVAL: Duration = Duration::from_millis(250);
pub const CONNECT_BUDGET: Duration = Duration::from_secs(10);
const STDERR_GRACE: Duration = Duration::from_millis(500);
const STDERR_POLL: Duration = Duration::from_millis(25);

pub fn ssh_bin() -> String {
    std::env::var(SSH_BIN_ENV)
        .ok()
        .filter(|bin| !bin.is_empty())
        .unwrap_or_else(|| DEFAULT_SSH_BIN.to_string())
}

type SshSaid = Arc<Mutex<Option<String>>>;

/// A live forward. It exists only while the ssh child does: hold it for as long
/// as requests must flow, and drop it to take the forward down.
pub struct Tunnel {
    child: Child,
    said: SshSaid,
    pub local_port: u16,
}

impl std::fmt::Debug for Tunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tunnel")
            .field("local_port", &self.local_port)
            .finish()
    }
}

impl Tunnel {
    /// Open a forward to `destination`'s `remote_port` and return once it
    /// carries traffic. Blocks for up to [`CONNECT_BUDGET`]; every failure —
    /// ssh missing, ssh dead, no forward in time — names the destination and
    /// quotes whatever ssh said.
    pub fn open(
        destination: &str,
        ssh_port: Option<u16>,
        remote_port: u16,
        ssh_args: &[String],
        ssh_bin: &str,
    ) -> Result<Tunnel> {
        Tunnel::open_within(
            destination,
            ssh_port,
            remote_port,
            ssh_args,
            ssh_bin,
            CONNECT_BUDGET,
        )
    }

    /// [`Tunnel::open`] with the wait budget injected, so a test can watch an
    /// ssh that never opens a forward give up in well under ten seconds.
    pub fn open_within(
        destination: &str,
        ssh_port: Option<u16>,
        remote_port: u16,
        ssh_args: &[String],
        ssh_bin: &str,
        budget: Duration,
    ) -> Result<Tunnel> {
        let local_port = free_loopback_port()?;
        let mut command = Command::new(ssh_bin);
        command
            .arg("-N")
            .arg("-o")
            .arg(EXIT_ON_FORWARD_FAILURE)
            .arg("-L")
            .arg(format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}"));
        if let Some(port) = ssh_port {
            command.arg("-p").arg(port.to_string());
        }
        let mut child = command
            .args(ssh_args)
            .arg(destination)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawn {ssh_bin} — is it on PATH?"))?;
        let said = match child.stderr.take() {
            Some(stderr) => collect(stderr),
            None => Arc::new(Mutex::new(Some(String::new()))),
        };
        let mut tunnel = Tunnel {
            child,
            said,
            local_port,
        };
        tunnel.await_forward(destination, budget)?;
        Ok(tunnel)
    }

    fn await_forward(&mut self, destination: &str, budget: Duration) -> Result<()> {
        let deadline = Instant::now() + budget;
        loop {
            let answering =
                TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.local_port)))
                    .is_ok();
            let exited = self
                .child
                .try_wait()
                .context("wait on the ssh forward process")?;
            match (answering, exited) {
                (true, None) => return Ok(()),
                (true, Some(status)) => bail!(
                    "ssh to {destination} {}, so 127.0.0.1:{} is not its forward — something else took the port{}",
                    exited_as(status),
                    self.local_port,
                    self.stderr_tail()
                ),
                (false, Some(status)) => bail!(
                    "ssh to {destination} {} before the forward opened{}",
                    exited_as(status),
                    self.stderr_tail()
                ),
                (false, None) => {}
            }
            if Instant::now() >= deadline {
                self.terminate();
                bail!(
                    "ssh to {destination} opened no forward within {}s{}",
                    budget.as_secs_f32(),
                    self.stderr_tail()
                );
            }
            std::thread::sleep(CONNECT_INTERVAL);
        }
    }

    fn stderr_tail(&self) -> String {
        let deadline = Instant::now() + STDERR_GRACE;
        loop {
            if let Ok(said) = self.said.lock() {
                if let Some(said) = said.as_deref() {
                    return match said.trim() {
                        "" => String::new(),
                        said => format!(": {said}"),
                    };
                }
            }
            if Instant::now() >= deadline {
                return String::new();
            }
            std::thread::sleep(STDERR_POLL);
        }
    }

    fn terminate(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.terminate();
    }
}

fn collect(mut stderr: ChildStderr) -> SshSaid {
    let said: SshSaid = Arc::new(Mutex::new(None));
    let sink = said.clone();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        if let Ok(mut sink) = sink.lock() {
            *sink = Some(text);
        }
    });
    said
}

fn exited_as(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exited {code}"),
        None => "was killed".to_string(),
    }
}

// NOTE: this reserves nothing — the listener is closed before ssh binds, so
// another process can take the port in between. `ExitOnForwardFailure=yes` and
// the answering-while-alive check in `await_forward` are what make that window
// a reported failure instead of a request sent to whoever won it.
fn free_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .context("reserve a loopback port for the ssh forward")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    const PORT_LOTTERY_RETRIES: usize = 8;

    // `GOLEMCTL_SSH` is not read here — each test passes its fake ssh
    // explicitly — but the picked loopback port is a shared resource, so
    // spawning one at a time keeps two tests from racing for the same one.
    static ONE_SPAWN_AT_A_TIME: Mutex<()> = Mutex::new(());

    fn one_spawn_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        ONE_SPAWN_AT_A_TIME
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn fake_ssh(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    // These tests assert on how a doomed ssh is *reported*, so they need the
    // failure, not the lottery: if the freshly-picked port is claimed by
    // something else before ssh reaches it, `open` can return a tunnel to that
    // stranger instead. Retrying re-rolls the port until the intended failure
    // is the one observed.
    fn failure_of(mut open: impl FnMut() -> Result<Tunnel>) -> String {
        for _ in 0..PORT_LOTTERY_RETRIES {
            match open() {
                Err(err) => return format!("{err:#}"),
                Ok(tunnel) => drop(tunnel),
            }
        }
        panic!("this ssh never reported a failure in {PORT_LOTTERY_RETRIES} tries")
    }

    #[test]
    fn the_forward_is_spawned_with_the_local_and_remote_loopback_ports() {
        let _serial = one_spawn_at_a_time();
        let dir = tempfile::tempdir().unwrap();
        let argv = dir.path().join("argv");
        let ssh = fake_ssh(
            dir.path(),
            "record",
            &format!("printf '%s\\n' \"$@\" > {}\nexit 3", argv.display()),
        );
        let err = failure_of(|| {
            Tunnel::open(
                "golem@scaly",
                Some(2222),
                7474,
                &["-i".to_string(), "/keys/id".to_string()],
                ssh.to_str().unwrap(),
            )
        });
        let recorded: Vec<String> = std::fs::read_to_string(&argv)
            .unwrap_or_else(|e| panic!("no argv recorded ({e}); ssh failed with: {err}"))
            .lines()
            .map(|line| line.to_string())
            .collect();
        assert_eq!(recorded[0], "-N");
        assert_eq!(recorded[1..3], ["-o", EXIT_ON_FORWARD_FAILURE]);
        assert_eq!(recorded[3], "-L");
        let spec: Vec<&str> = recorded[4].split(':').collect();
        assert_eq!(spec[0], "127.0.0.1");
        assert!(spec[1].parse::<u16>().unwrap() > 0);
        assert_eq!(spec[2], "127.0.0.1");
        assert_eq!(spec[3], "7474");
        assert_eq!(
            recorded[5..],
            ["-p", "2222", "-i", "/keys/id", "golem@scaly"]
        );
        assert!(err.contains("golem@scaly"), "{err}");
    }

    #[test]
    fn an_ssh_that_dies_before_the_forward_opens_carries_its_stderr() {
        let _serial = one_spawn_at_a_time();
        let dir = tempfile::tempdir().unwrap();
        let ssh = fake_ssh(
            dir.path(),
            "refuse",
            "echo 'ssh: connect to host scaly port 22: No route to host' >&2\nexit 255",
        );
        let err =
            failure_of(|| Tunnel::open("golem@scaly", None, 7474, &[], ssh.to_str().unwrap()));
        assert!(err.contains("No route to host"), "{err}");
        assert!(err.contains("golem@scaly"), "{err}");
    }

    #[test]
    fn an_ssh_that_never_opens_the_port_fails_when_the_budget_runs_out() {
        let _serial = one_spawn_at_a_time();
        let dir = tempfile::tempdir().unwrap();
        let ssh = fake_ssh(dir.path(), "hang", "sleep 30");
        let err = failure_of(|| {
            Tunnel::open_within(
                "golem@scaly",
                None,
                7474,
                &[],
                ssh.to_str().unwrap(),
                Duration::from_millis(600),
            )
        });
        assert!(err.contains("golem@scaly"), "{err}");
        assert!(err.contains("forward"), "{err}");
    }

    #[test]
    fn a_missing_ssh_binary_names_the_binary_it_could_not_spawn() {
        let _serial = one_spawn_at_a_time();
        let err = failure_of(|| Tunnel::open("golem@scaly", None, 7474, &[], "/nonexistent/ssh"));
        assert!(err.contains("/nonexistent/ssh"), "{err}");
    }
}
