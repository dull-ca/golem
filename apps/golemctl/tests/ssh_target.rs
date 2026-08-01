//! End-to-end cover for ssh targets (ADR 0042): a verb reaches a loopback-bound,
//! token-gated daemon through a forward golemctl opens, and that forward dies
//! with the verb.
//!
//! Real ssh cannot appear in a test, so `GOLEMCTL_SSH` points at a shell script
//! standing in for it. A script alone cannot forward, so the script re-execs
//! *this test binary* at `the_fake_ssh_forwarder_re_exec_of_this_test_binary` —
//! an `#[ignore]`d test that is not a test, but a TCP pump from the `-L` spec's
//! local port to its remote one. That keeps the fake forwarder in the same
//! source file as the tests it serves, and needs no second binary in the crate.
//!
//! `SSH_BIN_LOCK` serializes these tests: `GOLEMCTL_SSH` is process-wide, and
//! cargo runs a file's tests on threads of one process.

use std::io::copy;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use golemctl::conn::{AuthSource, Conn};
use golemctl::fleet;
use golemctl::inventory::{self, Endpoint, Target};
use golemctl::tunnel::{Tunnel, SSH_BIN_ENV};
use golemd::fake_reconciler::FakeReconciler;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::planroom::MemoryPlanRoom;

const FORWARD_SPEC_ENV: &str = "GOLEMCTL_FAKE_SSH_FORWARD";
const FORWARD_REMOTE_ENV: &str = "GOLEMCTL_FAKE_SSH_REMOTE";
const FORWARD_BOUND_ENV: &str = "GOLEMCTL_FAKE_SSH_BOUND";
const FORWARDER_TEST: &str = "the_fake_ssh_forwarder_re_exec_of_this_test_binary";

static SSH_BIN_LOCK: Mutex<()> = Mutex::new(());

struct FakeSsh {
    dir: tempfile::TempDir,
    _lock: MutexGuard<'static, ()>,
}

impl FakeSsh {
    fn standing_in_for_ssh(body: &str) -> FakeSsh {
        FakeSsh::written(|_| body.to_string())
    }

    fn forwarding() -> FakeSsh {
        FakeSsh::written(|dir| forwarder_script(&dir.join("pid")))
    }

    fn losing_the_port_to_a_squatter() -> FakeSsh {
        FakeSsh::written(squatter_script)
    }

    fn persisting_the_forward_into_a_background_master() -> FakeSsh {
        FakeSsh::written(persisted_master_script)
    }

    fn written(body: impl Fn(&Path) -> String) -> FakeSsh {
        let lock = SSH_BIN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ssh");
        std::fs::write(&path, format!("#!/bin/sh\n{}\n", body(dir.path()))).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var(SSH_BIN_ENV, &path);
        FakeSsh { dir, _lock: lock }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}

impl Drop for FakeSsh {
    fn drop(&mut self) {
        std::env::remove_var(SSH_BIN_ENV);
    }
}

fn forwarder_script(pidfile: &Path) -> String {
    let binary = std::env::current_exe().unwrap();
    format!(
        r#"echo $$ > "{pidfile}"
while [ $# -gt 0 ] && [ "$1" != "-L" ]; do shift; done
[ $# -lt 2 ] && exit 9
{FORWARD_SPEC_ENV}="$2" exec "{binary}" --exact {FORWARDER_TEST} --ignored --nocapture"#,
        pidfile = pidfile.display(),
        binary = binary.display(),
    )
}

// The hazard `ExitOnForwardFailure` and the answering-while-alive check exist
// for: this "ssh" leaves a forwarder holding the local port and then exits
// nonzero, so the port answers while ssh is dead. It waits for the squatter to
// have bound before exiting, so the race is staged rather than hoped for.
fn squatter_script(dir: &Path) -> String {
    let binary = std::env::current_exe().unwrap();
    format!(
        r#"while [ $# -gt 0 ] && [ "$1" != "-L" ]; do shift; done
{FORWARD_SPEC_ENV}="$2" {FORWARD_BOUND_ENV}="{bound}" "{binary}" --exact {FORWARDER_TEST} --ignored --nocapture &
echo $! > "{squatter}"
while [ ! -f "{bound}" ]; do :; done
exit 255"#,
        bound = dir.join("bound").display(),
        squatter = dir.join("squatter").display(),
        binary = binary.display(),
    )
}

// ControlMaster + ControlPersist: this "ssh" leaves a forwarder holding the
// local port, hands it to a master that outlives the client, and exits 0 — and
// takes the forward down again when it is asked to `-O cancel` it.
fn persisted_master_script(dir: &Path) -> String {
    let binary = std::env::current_exe().unwrap();
    format!(
        r#"if [ "$1" = -O ]; then kill $(cat "{pid}"); exit 0; fi
while [ $# -gt 0 ] && [ "$1" != "-L" ]; do shift; done
[ $# -lt 2 ] && exit 9
{FORWARD_SPEC_ENV}="$2" {FORWARD_BOUND_ENV}="{bound}" "{binary}" --exact {FORWARDER_TEST} --ignored --nocapture &
echo $! > "{pid}"
while [ ! -f "{bound}" ]; do :; done
exit 0"#,
        pid = dir.join("pid").display(),
        bound = dir.join("bound").display(),
        binary = binary.display(),
    )
}

#[test]
#[ignore = "not a test — the fake ssh forwarder the ssh targets re-exec"]
fn the_fake_ssh_forwarder_re_exec_of_this_test_binary() {
    let Ok(spec) = std::env::var(FORWARD_SPEC_ENV) else {
        return;
    };
    let ports: Vec<&str> = spec.split(':').collect();
    let local: u16 = ports[1].parse().unwrap();
    let remote: u16 = match std::env::var(FORWARD_REMOTE_ENV) {
        Ok(overridden) => overridden.parse().unwrap(),
        Err(_) => ports[3].parse().unwrap(),
    };
    let server = TcpListener::bind(("127.0.0.1", local)).unwrap();
    if let Ok(bound) = std::env::var(FORWARD_BOUND_ENV) {
        std::fs::write(bound, "bound").unwrap();
    }
    for downstream in server.incoming() {
        let Ok(downstream) = downstream else { continue };
        let Ok(upstream) = TcpStream::connect(("127.0.0.1", remote)) else {
            continue;
        };
        pump(
            downstream.try_clone().unwrap(),
            upstream.try_clone().unwrap(),
        );
        pump(upstream, downstream);
    }
}

fn pump(mut from: TcpStream, mut to: TcpStream) {
    std::thread::spawn(move || {
        let _ = copy(&mut from, &mut to);
        let _ = to.shutdown(Shutdown::Write);
    });
}

const REFUSING_SSH: &str =
    "echo 'ssh: connect to host scaly port 22: No route to host' >&2\nexit 255";

async fn serve_gated(host: &str, token: Option<&str>) -> u16 {
    let foreman = Foreman::new(
        host.to_string(),
        Box::new(MemoryPlanRoom::new()),
        Box::new(FakeReconciler::new()),
    );
    let app = http::router(http::AppState {
        foreman: Arc::new(foreman),
        required_token: token.map(|t| Arc::new(t.to_string())),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    port
}

fn ssh_inventory(
    dir: &Path,
    host: &str,
    remote_port: u16,
    token_file: Option<&Path>,
) -> Vec<Target> {
    let mut text = format!(
        "[hosts.{host}]\nssh = \"golem@127.0.0.1\"\nssh_port = 2222\nremote_port = {remote_port}\n"
    );
    if let Some(path) = token_file {
        text.push_str(&format!("token_file = \"{}\"\n", path.display()));
    }
    let path = dir.join("fleet.toml");
    std::fs::write(&path, text).unwrap();
    inventory::load(&path).unwrap().select(None).unwrap()
}

fn forward_pid(ssh: &FakeSsh) -> String {
    std::fs::read_to_string(ssh.path("pid"))
        .unwrap()
        .trim()
        .to_string()
}

fn lives(pid: &str) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn await_death(pid: &str) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while lives(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    !lives(pid)
}

#[test]
fn the_forward_is_open_while_the_tunnel_lives_and_gone_once_it_drops() {
    let ssh = FakeSsh::forwarding();
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let remote_port = listener.local_addr().unwrap().port();

    let tunnel = Tunnel::open(
        "golem@127.0.0.1",
        None,
        remote_port,
        &[],
        ssh.path("ssh").to_str().unwrap(),
    )
    .unwrap();
    let local_port = tunnel.local_port;
    assert!(TcpStream::connect(("127.0.0.1", local_port)).is_ok());

    let pid = forward_pid(&ssh);
    assert!(lives(&pid));
    drop(tunnel);
    assert!(await_death(&pid), "the forward dies with the tunnel");
    assert!(TcpStream::connect(("127.0.0.1", local_port)).is_err());
}

#[test]
fn a_port_answering_after_ssh_died_is_a_squatter_not_the_forward() {
    let ssh = FakeSsh::losing_the_port_to_a_squatter();
    let err = Tunnel::open(
        "golem@scaly",
        None,
        7474,
        &[],
        ssh.path("ssh").to_str().unwrap(),
    )
    .unwrap_err()
    .to_string();
    kill(&std::fs::read_to_string(ssh.path("squatter")).unwrap());

    assert!(err.contains("golem@scaly"), "{err}");
    assert!(err.contains("exited 255"), "{err}");
    assert!(err.contains("took the port"), "{err}");
}

fn kill(pid: &str) {
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(format!("kill {}", pid.trim()))
        .status()
        .unwrap();
}

#[tokio::test]
async fn an_ssh_target_reaches_a_loopback_gated_daemon_through_the_forward() {
    let ssh = FakeSsh::forwarding();
    let token_file = ssh.path("token");
    std::fs::write(&token_file, "secret\n").unwrap();
    let remote_port = serve_gated("scaly", Some("secret")).await;
    let targets = ssh_inventory(ssh.dir.path(), "scaly", remote_port, Some(&token_file));

    assert_eq!(
        targets[0].endpoint,
        Endpoint::Ssh {
            destination: "golem@127.0.0.1".into(),
            ssh_port: Some(2222),
            remote_port,
            ssh_args: vec![],
        }
    );

    let conn = Conn::open(&targets[0], &AuthSource::None).await.unwrap();
    let status = conn.get_json("status").await.unwrap();
    assert_eq!(status["host"], "scaly");

    let pid = forward_pid(&ssh);
    assert!(lives(&pid), "the forward runs while the verb does");
    drop(conn);
    assert!(
        await_death(&pid),
        "the forward dies with the conn that opened it"
    );
}

#[tokio::test]
async fn a_verb_crosses_a_forward_whose_ssh_handed_it_to_a_persisted_master() {
    let ssh = FakeSsh::persisting_the_forward_into_a_background_master();
    let token_file = ssh.path("token");
    std::fs::write(&token_file, "secret\n").unwrap();
    let remote_port = serve_gated("scaly", Some("secret")).await;
    let targets = ssh_inventory(ssh.dir.path(), "scaly", remote_port, Some(&token_file));

    let conn = Conn::open(&targets[0], &AuthSource::None)
        .await
        .unwrap_or_else(|e| panic!("a persisted master's forward was rejected: {e:#}"));
    let status = conn.get_json("status").await.unwrap();
    assert_eq!(status["host"], "scaly");

    let pid = forward_pid(&ssh);
    assert!(
        lives(&pid),
        "the persisted forward runs while the verb does"
    );
    drop(conn);
    assert!(
        await_death(&pid),
        "the persisted forward is cancelled through the master when the conn drops"
    );
}

#[tokio::test]
async fn a_forwarded_verb_without_the_secret_is_still_refused() {
    let ssh = FakeSsh::forwarding();
    let remote_port = serve_gated("scaly", Some("secret")).await;
    let targets = ssh_inventory(ssh.dir.path(), "scaly", remote_port, None);
    let conn = Conn::open(&targets[0], &AuthSource::None).await.unwrap();
    let err = format!("{:#}", conn.get_json("status").await.unwrap_err());
    assert!(err.contains("GOLEM_AUTH_TOKEN"), "{err}");
}

// A rolled-back apply leaves through `std::process::exit`, which runs no
// destructors — only the real binary can show that the forward still comes
// down, so this drives `golemctl` as a subprocess and watches ssh's pid die.
#[test]
fn an_apply_that_exits_nonzero_still_takes_its_forward_down_with_it() {
    let ssh = FakeSsh::forwarding();
    let daemon = TcpListener::bind("127.0.0.1:0").unwrap();
    let daemon_port = daemon.local_addr().unwrap().port();
    std::thread::spawn(move || serve_a_rolled_back_apply(daemon));

    let manifest = ssh.path("manifest.bin");
    std::fs::write(&manifest, b"\x00").unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_golemctl"))
        .args(["apply", manifest.to_str().unwrap(), "ssh://golem@127.0.0.1"])
        .env(SSH_BIN_ENV, ssh.path("ssh"))
        .env(FORWARD_REMOTE_ENV, daemon_port.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("apply rolled_back"),
        "the nonzero exit is the report's, not a transport failure: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let pid = forward_pid(&ssh);
    assert!(
        await_death(&pid),
        "the forward outlived the verb that exited {:?}",
        output.status.code()
    );
}

fn serve_a_rolled_back_apply(listener: TcpListener) {
    while let Ok((mut stream, _)) = listener.accept() {
        let request = read_request(&mut stream);
        if request.is_empty() {
            continue;
        }
        let (status, body) = if request.starts_with("POST /manifest") {
            ("202 Accepted", r#"{"reconcile_id":1}"#.to_string())
        } else {
            (
                "200 OK",
                serde_json::json!({
                    "reconcile_id": 1,
                    "phase": "rolled_back",
                    "units": [],
                    "events": [],
                    "cursor": 1,
                    "report": {
                        "outcome": "rolled_back",
                        "revision": { "id": 1 },
                        "units": []
                    }
                })
                .to_string(),
            )
        };
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = std::io::Write::write_all(&mut stream, response.as_bytes());
        let _ = std::io::Write::flush(&mut stream);
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut buf = [0u8; 8192];
    let mut text = String::new();
    loop {
        let read = std::io::Read::read(stream, &mut buf).unwrap_or(0);
        if read == 0 {
            break;
        }
        text.push_str(&String::from_utf8_lossy(&buf[..read]));
        if text.contains("\r\n\r\n") {
            break;
        }
    }
    text
}

#[tokio::test]
async fn a_fleet_status_over_a_failing_ssh_reports_what_ssh_said() {
    let _ssh = FakeSsh::standing_in_for_ssh(REFUSING_SSH);
    let reachable_port = serve_gated("manta", None).await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fleet.toml");
    std::fs::write(
        &path,
        format!(
            "[hosts.manta]\nurl = \"http://127.0.0.1:{reachable_port}\"\n\n[hosts.scaly]\nssh = \"golem@scaly\"\n"
        ),
    )
    .unwrap();
    let targets = inventory::load(&path).unwrap().select(None).unwrap();

    let readings = fleet::gather_status(&targets, &AuthSource::None).await;
    let lines = fleet::status_lines(&readings, false);

    assert!(lines[0].starts_with("· manta"), "{lines:?}");
    assert!(lines[1].contains("unreachable:"), "{lines:?}");
    assert!(lines[1].contains("No route to host"), "{lines:?}");

    let aggregate = fleet::status_json(&readings);
    assert_eq!(aggregate["hosts"]["scaly"]["addr"], "ssh://golem@scaly");
}
