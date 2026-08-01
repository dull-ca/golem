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
//! **Lifecycle, and the race it does not fully close.** `free_loopback_port`
//! binds port 0, reads what the kernel chose, and closes it — the port is then
//! merely *likely* free, and the window between closing it and ssh binding it
//! is a real race. `ExitOnForwardFailure=yes` narrows that window: an ssh that
//! finds the port taken exits instead of running on with no forward. It does
//! not close it, because ssh binds the `-L` listener only *after* it
//! authenticates — 100ms to seconds on a cold connection — while its command
//! line, chosen port and all, is world-readable in `/proc` the whole time. So
//! `await_forward` requires the port to answer *while ssh is still alive* on
//! two probes [`CONNECT_INTERVAL`] apart, and [`Tunnel::confirm_alive`] checks
//! the child once more immediately before the first request. That catches the
//! losing ssh whenever its bind attempt falls between two probes — the ordinary
//! case, a refused bind exiting within milliseconds of being refused while the
//! probes stand a quarter-second apart — and the port it was denied is then
//! reported as the stranger it is rather than trusted. It is a narrowing, not a
//! guarantee.
//!
//! **A dead ssh over an answering port is two different events, told apart by
//! its exit code.** Under `ControlMaster` + `ControlPersist` — the stanza golem's
//! own docs hand out — an ssh that finds no master forks one into the background
//! and the foreground `-N` client, left with nothing to do, exits *0* the instant
//! the forward is up. The forward is real; the process golemctl spawned is simply
//! no longer the thing holding it. A *lost* bind cannot reach that exit: with
//! `ExitOnForwardFailure=yes` ssh exits 255 on a refused `-L` bind, whether it
//! would have become a master, was already talking to one, or was multiplexing
//! nothing at all. So a zero exit over an answering port is a forward handed to a
//! persisted master and is used; any other exit over an answering port is still
//! the squatter above, still reported with ssh's own stderr.
//!
//! What survives is a residual window. A local process that reads the port from
//! `/proc` and binds it while ssh is still authenticating answers every probe
//! and keeps ssh alive on every one of them, so golemctl cannot tell it from a
//! working forward and will send it the bearer token. Closing that needs a
//! rendezvous only the owner can open — forwarding a unix socket rather than a
//! TCP port — which ADR 0042 records as the future path; the window is accepted
//! until then. [`Drop`] kills the child and reaps it, so the forward never
//! outlives the verb even when the verb exits nonzero. Killing a child that
//! already handed its forward to a persisted master would reclaim nothing, so
//! that forward is instead cancelled through the master — `ssh -O cancel -L`
//! with the same spec — which frees the port and leaves the master itself up,
//! the reuse being the whole point of the operator's `ControlPersist`. The
//! cancel is bounded by `CANCEL_GRACE`: a wedged master delays the verb's exit
//! by that much, it does not hang it.
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
const CANCEL_GRACE: Duration = Duration::from_secs(2);
const CANCEL_POLL: Duration = Duration::from_millis(20);

pub fn ssh_bin() -> String {
    std::env::var(SSH_BIN_ENV)
        .ok()
        .filter(|bin| !bin.is_empty())
        .unwrap_or_else(|| DEFAULT_SSH_BIN.to_string())
}

type SshSaid = Arc<Mutex<Option<String>>>;

struct Ssh {
    bin: String,
    destination: String,
    port: Option<u16>,
    args: Vec<String>,
}

impl Ssh {
    fn command(&self, verb: &[&str], forward: &str) -> Command {
        let mut command = Command::new(&self.bin);
        command.args(verb).arg("-L").arg(forward);
        if let Some(port) = self.port {
            command.arg("-p").arg(port.to_string());
        }
        command.args(&self.args).arg(&self.destination);
        command
    }
}

enum HeldBy {
    TheSshChild,
    APersistedMaster,
}

/// A live forward. Hold it for as long as requests must flow, and drop it to
/// take the forward down — whether the ssh child still carries it or a
/// persisted master was handed it.
pub struct Tunnel {
    child: Child,
    said: SshSaid,
    ssh: Ssh,
    forward: String,
    held_by: HeldBy,
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
        let ssh = Ssh {
            bin: ssh_bin.to_string(),
            destination: destination.to_string(),
            port: ssh_port,
            args: ssh_args.to_vec(),
        };
        let forward = format!("127.0.0.1:{local_port}:127.0.0.1:{remote_port}");
        let mut child = ssh
            .command(&["-N", "-o", EXIT_ON_FORWARD_FAILURE], &forward)
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
            ssh,
            forward,
            held_by: HeldBy::TheSshChild,
            local_port,
        };
        tunnel.await_forward(budget)?;
        Ok(tunnel)
    }

    pub fn confirm_alive(&mut self) -> Result<()> {
        if let HeldBy::APersistedMaster = self.held_by {
            return self.confirm_the_master_still_answers();
        }
        match self
            .child
            .try_wait()
            .context("wait on the ssh forward process")?
        {
            None => Ok(()),
            Some(status) => bail!(
                "ssh to {} {} before the first request crossed its forward, so 127.0.0.1:{} is no longer it{}",
                self.ssh.destination,
                exited_as(status),
                self.local_port,
                self.stderr_tail()
            ),
        }
    }

    fn confirm_the_master_still_answers(&self) -> Result<()> {
        match TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.local_port))) {
            Ok(_) => Ok(()),
            Err(_) => bail!(
                "the ssh master holding the forward to {} stopped answering 127.0.0.1:{} before the first request crossed it{}",
                self.ssh.destination,
                self.local_port,
                self.stderr_tail()
            ),
        }
    }

    fn await_forward(&mut self, budget: Duration) -> Result<()> {
        let destination = self.ssh.destination.clone();
        let deadline = Instant::now() + budget;
        let mut carried_on_the_previous_probe = false;
        loop {
            let answering =
                TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.local_port)))
                    .is_ok();
            let exited = self
                .child
                .try_wait()
                .context("wait on the ssh forward process")?;
            match (answering, exited) {
                (true, None) if carried_on_the_previous_probe => return Ok(()),
                (true, None) => carried_on_the_previous_probe = true,
                (true, Some(status)) if status.success() => {
                    self.held_by = HeldBy::APersistedMaster;
                    return Ok(());
                }
                (true, Some(status)) => bail!(
                    "ssh to {destination} {}, so 127.0.0.1:{} is not its forward — something else took the port{}",
                    exited_as(status),
                    self.local_port,
                    self.stderr_tail()
                ),
                (false, Some(status)) if status.success() => {
                    carried_on_the_previous_probe = false
                }
                (false, Some(status)) => bail!(
                    "ssh to {destination} {} before the forward opened{}",
                    exited_as(status),
                    self.stderr_tail()
                ),
                (false, None) => carried_on_the_previous_probe = false,
            }
            if !carried_on_the_previous_probe && Instant::now() >= deadline {
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

    fn cancel_through_the_master(&self) {
        let Ok(mut cancel) = self
            .ssh
            .command(&["-O", "cancel"], &self.forward)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        let deadline = Instant::now() + CANCEL_GRACE;
        loop {
            match cancel.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) if Instant::now() >= deadline => break,
                Ok(None) => std::thread::sleep(CANCEL_POLL),
            }
        }
        let _ = cancel.kill();
        let _ = cancel.wait();
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        if let HeldBy::APersistedMaster = self.held_by {
            self.cancel_through_the_master();
        }
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
// another process can take the port in between, and ssh publishes which port to
// take in its `/proc` command line. `ExitOnForwardFailure=yes` plus the
// two-probe answering-while-alive check in `await_forward` turn a *lost* bind
// into a reported failure; they cannot turn a squatter that wins the port and
// holds it into one. See the module docs for the window that leaves open.
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
    use std::sync::atomic::{AtomicBool, Ordering};
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

    fn squat_the_forwarded_port(argv: PathBuf, stand_down: PathBuf) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let listener = loop {
                if Instant::now() >= deadline {
                    return;
                }
                if let Some(port) = recorded_forward_port(&argv) {
                    if let Ok(listener) =
                        TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
                    {
                        break listener;
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            listener.set_nonblocking(true).unwrap();
            let mut told_ssh_to_exit = false;
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok(_) if !told_ssh_to_exit => {
                        std::fs::write(&stand_down, "").unwrap();
                        told_ssh_to_exit = true;
                    }
                    Ok(_) => {}
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        })
    }

    fn recorded_forward_port(argv: &Path) -> Option<u16> {
        let recorded = std::fs::read_to_string(argv).ok()?;
        let spec = recorded.lines().nth(4)?;
        spec.split(':').nth(1)?.parse().ok()
    }

    #[test]
    fn a_port_squatter_that_outlives_the_ssh_it_displaced_is_reported_not_used() {
        let _serial = one_spawn_at_a_time();
        let dir = tempfile::tempdir().unwrap();
        let argv = dir.path().join("argv");
        let stand_down = dir.path().join("stand-down");
        let ssh = fake_ssh(
            dir.path(),
            "displaced",
            &format!(
                "printf '%s\\n' \"$@\" > {}\nwhile [ ! -f {} ]; do sleep 0.02; done\nexit 255",
                argv.display(),
                stand_down.display()
            ),
        );
        let squatter = squat_the_forwarded_port(argv.clone(), stand_down.clone());
        let err = Tunnel::open_within(
            "golem@scaly",
            None,
            7474,
            &[],
            ssh.to_str().unwrap(),
            Duration::from_secs(5),
        )
        .map(|_| ())
        .unwrap_err();
        let err = format!("{err:#}");
        assert!(err.contains("golem@scaly"), "{err}");
        assert!(err.contains("something else took the port"), "{err}");
        drop(squatter);
    }

    fn keep_the_forwarded_port(
        argv: PathBuf,
        bound: PathBuf,
        stop: Arc<AtomicBool>,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let listener = loop {
                if Instant::now() >= deadline {
                    return;
                }
                if let Some(port) = recorded_forward_port(&argv) {
                    if let Ok(listener) =
                        TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
                    {
                        break listener;
                    }
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            listener.set_nonblocking(true).unwrap();
            std::fs::write(&bound, "").unwrap();
            while !stop.load(Ordering::SeqCst) && Instant::now() < deadline {
                if listener.accept().is_err() {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        })
    }

    struct HandedOff {
        ssh: PathBuf,
        argv: PathBuf,
        cancelled: PathBuf,
        stop: Arc<AtomicBool>,
        holder: Option<std::thread::JoinHandle<()>>,
    }

    impl HandedOff {
        fn to_a_master_and_then_exiting(dir: &Path, code: i32) -> HandedOff {
            let argv = dir.join("argv");
            let bound = dir.join("bound");
            let cancelled = dir.join("cancelled");
            let ssh = fake_ssh(
                dir,
                "persisted",
                &format!(
                    "if [ \"$1\" = -O ]; then printf '%s\\n' \"$@\" > {}; exit 0; fi\n\
                     printf '%s\\n' \"$@\" > {}\n\
                     while [ ! -f {} ]; do sleep 0.02; done\n\
                     exit {code}",
                    cancelled.display(),
                    argv.display(),
                    bound.display(),
                ),
            );
            let stop = Arc::new(AtomicBool::new(false));
            let holder = keep_the_forwarded_port(argv.clone(), bound, stop.clone());
            HandedOff {
                ssh,
                argv,
                cancelled,
                stop,
                holder: Some(holder),
            }
        }

        fn open(&self) -> Result<Tunnel> {
            Tunnel::open_within(
                "golem@scaly",
                None,
                7474,
                &[],
                self.ssh.to_str().unwrap(),
                Duration::from_secs(5),
            )
        }
    }

    impl Drop for HandedOff {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(holder) = self.holder.take() {
                let _ = holder.join();
            }
        }
    }

    #[test]
    fn a_forward_handed_to_a_persisted_master_that_exits_zero_is_used_not_rejected() {
        let _serial = one_spawn_at_a_time();
        let dir = tempfile::tempdir().unwrap();
        let handed_off = HandedOff::to_a_master_and_then_exiting(dir.path(), 0);

        let mut tunnel = handed_off
            .open()
            .unwrap_or_else(|e| panic!("a persisted master's forward was rejected: {e:#}"));

        assert!(
            TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, tunnel.local_port))).is_ok()
        );
        tunnel.confirm_alive().unwrap();
    }

    #[test]
    fn a_forward_whose_ssh_exits_nonzero_is_a_squatter_however_well_the_port_answers() {
        let _serial = one_spawn_at_a_time();
        let dir = tempfile::tempdir().unwrap();
        let handed_off = HandedOff::to_a_master_and_then_exiting(dir.path(), 255);

        let err = format!("{:#}", handed_off.open().map(|_| ()).unwrap_err());

        assert!(err.contains("golem@scaly"), "{err}");
        assert!(err.contains("something else took the port"), "{err}");
    }

    #[test]
    fn dropping_a_persisted_master_forward_cancels_it_through_the_master() {
        let _serial = one_spawn_at_a_time();
        let dir = tempfile::tempdir().unwrap();
        let handed_off = HandedOff::to_a_master_and_then_exiting(dir.path(), 0);
        let tunnel = handed_off.open().unwrap();
        let local_port = tunnel.local_port;
        let spec = std::fs::read_to_string(&handed_off.argv).unwrap();
        let spec = spec.lines().nth(4).unwrap().to_string();

        drop(tunnel);

        let cancelled: Vec<String> = std::fs::read_to_string(&handed_off.cancelled)
            .unwrap_or_else(|e| panic!("the handed-off forward was never cancelled ({e})"))
            .lines()
            .map(|line| line.to_string())
            .collect();
        assert_eq!(cancelled[..2], ["-O", "cancel"]);
        assert_eq!(cancelled[2], "-L");
        assert_eq!(cancelled[3], spec);
        assert_eq!(cancelled[4], "golem@scaly");
        assert!(spec.contains(&local_port.to_string()), "{spec}");
    }

    #[test]
    fn a_dead_ssh_is_named_when_liveness_is_confirmed_before_the_first_request() {
        let _serial = one_spawn_at_a_time();
        let dir = tempfile::tempdir().unwrap();
        let ssh = fake_ssh(dir.path(), "hang", "sleep 30");
        let mut tunnel = Tunnel {
            child: Command::new(ssh.to_str().unwrap())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
            said: Arc::new(Mutex::new(Some(String::new()))),
            ssh: Ssh {
                bin: ssh.to_str().unwrap().to_string(),
                destination: "golem@scaly".to_string(),
                port: None,
                args: vec![],
            },
            forward: "127.0.0.1:1:127.0.0.1:7474".to_string(),
            held_by: HeldBy::TheSshChild,
            local_port: 1,
        };
        tunnel.confirm_alive().unwrap();
        tunnel.terminate();
        let err = format!("{:#}", tunnel.confirm_alive().unwrap_err());
        assert!(err.contains("golem@scaly"), "{err}");
        assert!(err.contains("before the first request"), "{err}");
    }

    #[test]
    fn a_missing_ssh_binary_names_the_binary_it_could_not_spawn() {
        let _serial = one_spawn_at_a_time();
        let err = failure_of(|| Tunnel::open("golem@scaly", None, 7474, &[], "/nonexistent/ssh"));
        assert!(err.contains("/nonexistent/ssh"), "{err}");
    }
}
