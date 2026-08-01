use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use golemctl::conn::{AuthSource, Conn};
use golemctl::fleet;
use golemctl::inventory::{self, Endpoint, Target};
use golemctl::tunnel::SSH_BIN_ENV;
use golemd::fake_reconciler::FakeReconciler;
use golemd::foreman::Foreman;
use golemd::http;
use golemd::planroom::MemoryPlanRoom;

static SSH_BIN_LOCK: Mutex<()> = Mutex::new(());

struct FakeSsh {
    dir: tempfile::TempDir,
    _lock: MutexGuard<'static, ()>,
}

impl FakeSsh {
    fn standing_in_for_ssh(body: &str) -> FakeSsh {
        let lock = SSH_BIN_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ssh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
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

const FORWARDING_SSH: &str = r#"echo $$ > "$GOLEMCTL_TEST_PIDFILE"
exec python3 -c '
import socket, sys, threading
spec = sys.argv[sys.argv.index("-L") + 1].split(":")
local_port, remote_port = int(spec[1]), int(spec[3])
def pump(src, dst):
    try:
        while True:
            block = src.recv(65536)
            if not block:
                break
            dst.sendall(block)
    except OSError:
        pass
    finally:
        try:
            dst.shutdown(socket.SHUT_WR)
        except OSError:
            pass
server = socket.socket()
server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
server.bind(("127.0.0.1", local_port))
server.listen(16)
while True:
    downstream, _ = server.accept()
    upstream = socket.create_connection(("127.0.0.1", remote_port))
    threading.Thread(target=pump, args=(downstream, upstream), daemon=True).start()
    threading.Thread(target=pump, args=(upstream, downstream), daemon=True).start()
' "$@""#;

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

fn lives(pid: &str) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

/// The whole point of ADR 0042 §2: a host reached over ssh is a loopback-bound
/// daemon, and the verb speaks plain gated HTTP through a forward golemctl owns.
#[tokio::test]
async fn an_ssh_target_reaches_a_loopback_gated_daemon_through_the_forward() {
    let ssh = FakeSsh::standing_in_for_ssh(FORWARDING_SSH);
    let pidfile = ssh.path("pid");
    std::env::set_var("GOLEMCTL_TEST_PIDFILE", &pidfile);
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

    let pid = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .to_string();
    assert!(lives(&pid), "the forward runs while the verb does");
    drop(conn);
    let deadline = Instant::now() + Duration::from_secs(2);
    while lives(&pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        !lives(&pid),
        "the forward dies with the conn that opened it"
    );
    std::env::remove_var("GOLEMCTL_TEST_PIDFILE");
}

/// A missing token is still a 401 through the tunnel — the forward is the
/// transport, never the authorization (ADR 0042 §3).
#[tokio::test]
async fn a_forwarded_verb_without_the_secret_is_still_refused() {
    let ssh = FakeSsh::standing_in_for_ssh(FORWARDING_SSH);
    std::env::set_var("GOLEMCTL_TEST_PIDFILE", ssh.path("pid"));
    let remote_port = serve_gated("scaly", Some("secret")).await;
    let targets = ssh_inventory(ssh.dir.path(), "scaly", remote_port, None);
    let conn = Conn::open(&targets[0], &AuthSource::None).await.unwrap();
    let err = format!("{:#}", conn.get_json("status").await.unwrap_err());
    assert!(err.contains("GOLEM_AUTH_TOKEN"), "{err}");
    std::env::remove_var("GOLEMCTL_TEST_PIDFILE");
}

/// One host's dead ssh is that host's line, not the fleet's failure: the status
/// row reports it through the same `concise_error` an http host's refusal takes.
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
