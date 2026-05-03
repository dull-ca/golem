//! Shared test harness for systemd-in-container integration tests.
//!
//! Each test brings up a fresh `debian:trixie + systemd` container, mounts
//! the release-built `golemd` and `golemctl` binaries inside, copies a
//! signed bundle in, then runs the agent. Tests interact with the agent's
//! HTTP server on the exposed port and exec into the container for
//! systemctl/dpkg/sqlite3 assertions.
//!
//! Build the image once before running tests:
//!
//!     docker build -t golem-smoke:trixie crates/golemd/tests/fixtures
//!
//! Tests are `#[ignore]` so default `cargo test` doesn't try to run them.
//! Run explicitly:
//!
//!     cargo test -p golemd --test smoke_install_remove -- --ignored --nocapture

#![allow(dead_code)] // Helpers are used per-test; some unused per binary.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use testcontainers::{
    core::{CgroupnsMode, ContainerPort, ExecCommand, IntoContainerPort, Mount, WaitFor},
    runners::SyncRunner,
    Container, GenericImage, ImageExt,
};

pub const IMAGE: &str = "golem-smoke";
pub const TAG: &str = "trixie";
pub const AGENT_PORT: u16 = 7474;

pub fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR for golemd is .../golem/crates/golemd
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

pub fn release_binary(name: &str) -> PathBuf {
    let p = workspace_root().join("target").join("release").join(name);
    assert!(
        p.exists(),
        "{} not found — build first: cargo build --release -p {} -p golemctl",
        p.display(),
        name
    );
    p
}

/// A running container with golemd ready to receive HTTP pushes.
///
/// The agent is NOT started by this struct — tests start it via `exec_agent`
/// once they've staged the bundle and trusted-keys file. This makes the
/// crash-injection tests straightforward: each restart is just another
/// `exec_agent` call.
pub struct Harness {
    pub container: Container<GenericImage>,
}

impl Harness {
    pub fn start() -> Self {
        // Only golemd needs to live inside the container — signing happens
        // on the host via host_sign(), so golemctl is never invoked here.
        let golemd = release_binary("golemd");

        let image = GenericImage::new(IMAGE, TAG)
            .with_exposed_port(AGENT_PORT.tcp())
            // systemd needs a long while on cold-start; the wait condition is
            // satisfied when the test does its first `is-system-running --wait`.
            .with_wait_for(WaitFor::Duration {
                length: Duration::from_secs(2),
            });

        let mut req = image
            .with_privileged(true)
            // systemd inside Docker needs:
            //   - cgroupns=host so the container shares the host's cgroup
            //     hierarchy (otherwise systemd can't manage units),
            //   - tmpfs mounts on /run, /run/lock, /tmp,
            //   - /sys/fs/cgroup bind-mounted writable.
            // Without all four, /sbin/init exits 255 immediately.
            .with_cgroupns_mode(CgroupnsMode::Host)
            .with_mount(Mount::tmpfs_mount("/run"))
            .with_mount(Mount::tmpfs_mount("/run/lock"))
            .with_mount(Mount::tmpfs_mount("/tmp"))
            .with_mount(Mount::bind_mount("/sys/fs/cgroup", "/sys/fs/cgroup"))
            // Bind-mount the agent binary so test iteration doesn't rebuild the image.
            .with_mount(Mount::bind_mount(
                golemd.to_string_lossy().to_string(),
                "/usr/local/bin/golemd",
            ));

        // If the host is a Nix system, the release binary's dynamic linker
        // is `/nix/store/<hash>-glibc-…/lib/ld-linux-x86-64.so.2`. The
        // container has no /nix, so the binary fails with the classic
        // misleading "No such file or directory". Bind-mounting /nix/store
        // read-only makes the linker reachable. On non-Nix systems this
        // branch is a no-op (the directory simply doesn't exist).
        if std::path::Path::new("/nix/store").exists() {
            req = req.with_mount(Mount::bind_mount(
                "/nix/store".to_string(),
                "/nix/store",
            ));
        }

        let container = req
            .start()
            .expect("start container — is the image built? `docker build -t golem-smoke:trixie crates/golemd/tests/fixtures`");

        let h = Self { container };
        h.wait_for_systemd_ready();
        h
    }

    pub fn host_port(&self) -> u16 {
        self.container
            .get_host_port_ipv4(ContainerPort::Tcp(AGENT_PORT))
            .expect("port mapping")
    }

    pub fn agent_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.host_port())
    }

    fn wait_for_systemd_ready(&self) {
        // Poll without `--wait` — that flag blocks indefinitely if any unit
        // is in a non-final state (or failed permanently). We just want to
        // see one of: running / degraded / starting-but-progressing. The
        // states we accept are anything other than "offline" and "initializing".
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let (_, state) = self.exec(&["systemctl", "is-system-running"]);
            let s = state.trim();
            if matches!(s, "running" | "degraded" | "starting") {
                return;
            }
            if Instant::now() > deadline {
                panic!(
                    "systemd not ready inside container after 60s (state={s:?})"
                );
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    /// Run a one-shot command in the container, returning (exit_code, stdout).
    /// Panics on exec failure (not on non-zero exit code).
    pub fn exec(&self, args: &[&str]) -> (i64, String) {
        let mut result = self
            .container
            .exec(ExecCommand::new(args.iter().map(|s| s.to_string())))
            .expect("exec");
        let mut stdout = Vec::new();
        std::io::copy(&mut result.stdout(), &mut stdout).ok();
        let code = result.exit_code().ok().flatten().unwrap_or(-1);
        (code, String::from_utf8_lossy(&stdout).to_string())
    }

    /// Copy a host file into the container at the given absolute path.
    pub fn copy_in(&self, host: &Path, container_path: &str) {
        let id = self.container.id();
        let status = Command::new("docker")
            .args(["cp", &host.to_string_lossy(), &format!("{id}:{container_path}")])
            .status()
            .expect("docker cp");
        assert!(status.success(), "docker cp {} -> {} failed", host.display(), container_path);
    }

    /// Write a string to a file inside the container.
    pub fn write_file(&self, container_path: &str, contents: &str) {
        let tmp = tempfile::NamedTempFile::new().expect("tempfile");
        std::fs::write(tmp.path(), contents).expect("write tempfile");
        self.copy_in(tmp.path(), container_path);
    }

    /// Spawn the agent inside the container with the given env. Returns the
    /// in-container PID. Caller is responsible for monitoring exit.
    ///
    /// We use `nohup ... &` and capture the PID via $! to a known file, then
    /// read it back. The agent's stdout/stderr go to /tmp/golemd.log.
    pub fn spawn_agent(&self, env: &[(&str, &str)]) -> AgentHandle {
        let env_prefix = env
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" ");
        let cmd = format!(
            "{env_prefix} nohup /usr/local/bin/golemd \
                --node test-01 \
                --state-dir /var/lib/golem \
                --trusted-keys /etc/golem/trusted-keys \
                --bundle /etc/golem/bundle.json \
                --listen 0.0.0.0:{port} \
                --period-secs 5 \
                >/tmp/golemd.log 2>&1 & echo $! >/tmp/golemd.pid; wait $!",
            port = AGENT_PORT
        );
        // Don't wait — fire and forget; tests poll for liveness via HTTP.
        let id = self.container.id().to_string();
        std::thread::spawn(move || {
            let _ = Command::new("docker")
                .args(["exec", &id, "bash", "-lc", &cmd])
                .status();
        });
        // Spin until the pid file shows up.
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let (code, out) = self.exec(&["cat", "/tmp/golemd.pid"]);
            if code == 0 {
                if let Ok(pid) = out.trim().parse::<i64>() {
                    return AgentHandle { pid };
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("agent didn't write /tmp/golemd.pid within 10s");
    }

    /// Wait for the agent's HTTP /healthz to respond OK.
    pub fn wait_for_agent_ready(&self) {
        let url = format!("{}/healthz", self.agent_url());
        let deadline = Instant::now() + Duration::from_secs(30);
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .unwrap();
        while Instant::now() < deadline {
            if let Ok(resp) = client.get(&url).send() {
                if resp.status().is_success() {
                    return;
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        // Dump diagnostics before panicking.
        eprintln!("--- agent /healthz never came up at {url} ---");
        eprintln!("--- /tmp/golemd.pid ---\n{}", self.exec(&["cat", "/tmp/golemd.pid"]).1);
        eprintln!("--- ps in container ---\n{}", self.exec(&["ps", "axfo", "pid,stat,cmd"]).1);
        eprintln!("--- /tmp/golemd.log ---\n{}", self.dump_log());
        panic!("agent /healthz didn't come up within 30s");
    }

    pub fn agent_running(&self, h: &AgentHandle) -> bool {
        let (code, _) = self.exec(&["kill", "-0", &h.pid.to_string()]);
        code == 0
    }

    pub fn wait_for_agent_exit(&self, h: &AgentHandle, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if !self.agent_running(h) {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        panic!("agent (pid {}) did not exit within {timeout:?}", h.pid);
    }

    pub fn kill_agent(&self, h: &AgentHandle) {
        self.exec(&["kill", "-TERM", &h.pid.to_string()]);
        let _ = self.wait_for_agent_exit_lossy(h, Duration::from_secs(5));
    }

    fn wait_for_agent_exit_lossy(&self, h: &AgentHandle, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if !self.agent_running(h) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    }

    pub fn dump_log(&self) -> String {
        self.exec(&["cat", "/tmp/golemd.log"]).1
    }
}

pub struct AgentHandle {
    pub pid: i64,
}

/// Run golemctl on the host (not in the container) to keygen and sign.
/// We could also run them inside, but doing it on the host keeps the test
/// readable and reuses the same toolchain across tests.
pub fn host_keygen(stem: &Path) {
    let golemctl = release_binary("golemctl");
    let status = Command::new(&golemctl)
        .args(["keygen"])
        .arg(stem)
        .status()
        .expect("spawn golemctl keygen");
    assert!(status.success(), "golemctl keygen failed");
}

pub fn host_sign(bundle_json: &Path, sk: &Path) -> Vec<u8> {
    let golemctl = release_binary("golemctl");
    let out = Command::new(&golemctl)
        .args(["sign"])
        .arg(bundle_json)
        .arg(sk)
        .output()
        .expect("spawn golemctl sign");
    assert!(
        out.status.success(),
        "golemctl sign failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

pub fn smoke_test_dir() -> PathBuf {
    workspace_root().join("smoke-test")
}

pub fn post_bundle(url: &str, signed: &[u8]) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let resp = client
        .post(format!("{url}/bundle"))
        .header("content-type", "application/json")
        .body(signed.to_vec())
        .send()
        .map_err(|e| format!("send: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "POST /bundle: {} {}",
            resp.status(),
            resp.text().unwrap_or_default()
        ));
    }
    Ok(())
}
