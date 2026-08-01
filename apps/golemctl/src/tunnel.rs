use std::io::Read;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::process::{Child, ChildStderr, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

pub const SSH_BIN_ENV: &str = "GOLEMCTL_SSH";
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
            if TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, self.local_port))).is_ok()
            {
                return Ok(());
            }
            if let Some(status) = self
                .child
                .try_wait()
                .context("wait on the ssh forward process")?
            {
                bail!(
                    "ssh to {destination} {} before the forward opened{}",
                    exited_as(status),
                    self.stderr_tail()
                );
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
    use std::net::TcpStream;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    fn fake_ssh(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn the_forward_is_spawned_with_the_local_and_remote_loopback_ports() {
        let dir = tempfile::tempdir().unwrap();
        let argv = dir.path().join("argv");
        let ssh = fake_ssh(
            dir.path(),
            "record",
            &format!("printf '%s\\n' \"$@\" > {}\nexit 3", argv.display()),
        );
        let err = Tunnel::open(
            "golem@scaly",
            Some(2222),
            7474,
            &["-i".to_string(), "/keys/id".to_string()],
            ssh.to_str().unwrap(),
        )
        .unwrap_err();
        let recorded: Vec<String> = std::fs::read_to_string(&argv)
            .unwrap()
            .lines()
            .map(|line| line.to_string())
            .collect();
        assert_eq!(recorded[0], "-N");
        assert_eq!(recorded[1], "-L");
        let spec: Vec<&str> = recorded[2].split(':').collect();
        assert_eq!(spec[0], "127.0.0.1");
        assert!(spec[1].parse::<u16>().unwrap() > 0);
        assert_eq!(spec[2], "127.0.0.1");
        assert_eq!(spec[3], "7474");
        assert_eq!(
            recorded[3..],
            ["-p", "2222", "-i", "/keys/id", "golem@scaly"]
        );
        assert!(format!("{err:#}").contains("golem@scaly"), "{err:#}");
    }

    #[test]
    fn an_ssh_that_dies_before_the_forward_opens_carries_its_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let ssh = fake_ssh(
            dir.path(),
            "refuse",
            "echo 'ssh: connect to host scaly port 22: No route to host' >&2\nexit 255",
        );
        let err = Tunnel::open("golem@scaly", None, 7474, &[], ssh.to_str().unwrap())
            .unwrap_err()
            .to_string();
        assert!(err.contains("No route to host"), "{err}");
        assert!(err.contains("golem@scaly"), "{err}");
    }

    #[test]
    fn an_ssh_that_never_opens_the_port_fails_when_the_budget_runs_out() {
        let dir = tempfile::tempdir().unwrap();
        let ssh = fake_ssh(dir.path(), "hang", "sleep 30");
        let err = Tunnel::open_within(
            "golem@scaly",
            None,
            7474,
            &[],
            ssh.to_str().unwrap(),
            Duration::from_millis(600),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("golem@scaly"), "{err}");
        assert!(err.contains("forward"), "{err}");
    }

    #[test]
    fn a_missing_ssh_binary_names_the_binary_it_could_not_spawn() {
        let err = Tunnel::open("golem@scaly", None, 7474, &[], "/nonexistent/ssh")
            .unwrap_err()
            .to_string();
        assert!(err.contains("/nonexistent/ssh"), "{err}");
    }

    #[test]
    fn the_forward_is_open_while_the_tunnel_lives_and_gone_once_it_drops() {
        let dir = tempfile::tempdir().unwrap();
        let ssh = fake_ssh(
            dir.path(),
            "listen",
            "exec python3 -c 'import socket,sys,time\nspec=sys.argv[sys.argv.index(\"-L\")+1].split(\":\")\ns=socket.socket()\ns.bind((spec[0],int(spec[1])))\ns.listen(16)\ntime.sleep(30)' \"$@\"",
        );
        let tunnel = Tunnel::open("golem@scaly", None, 7474, &[], ssh.to_str().unwrap()).unwrap();
        let port = tunnel.local_port;
        assert!(TcpStream::connect(("127.0.0.1", port)).is_ok());
        drop(tunnel);
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    }
}
