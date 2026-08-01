//! One way to reach a daemon, whatever the inventory said (ADR 0042).
//!
//! Every verb — single-host and fleet alike — goes through [`Conn`]: it resolves
//! a [`Target`] to a base URL, opening an ssh forward first when the endpoint is
//! an ssh one ([`crate::tunnel`]), attaches the bearer token to each request,
//! and turns a `401` into an error naming the environment variables an operator
//! can set. Callers speak in paths (`status`, `manifest`, `reconciles/7`) and
//! never see whether the bytes crossed a tunnel or which credential carried
//! them.
//!
//! **Where the token comes from**, nearest source winning
//! ([`resolve_auth`]): the target's own inventory `token_file`, then
//! `GOLEM_AUTH_TOKEN`, then the file named by `GOLEM_AUTH_TOKEN_FILE`. A
//! per-host file overrides the ambient environment so one fan-out can span
//! hosts holding different secrets. Nothing found is [`AuthSource::None`] — no
//! header at all, which an ungated daemon accepts and a gated one refuses.
//! Reading an empty token file is an error rather than a silent `None`: a
//! truncated secret must not degrade into an unauthenticated request.
//!
//! **Where it is safe to send it.** ADR 0042 assumes the token only ever
//! crosses an ssh forward, and an `ssh://` target guarantees that — its base is
//! the loopback end of the tunnel. A plain `http://` target guarantees nothing:
//! aimed at a routable address it puts the fleet's shared secret on the network
//! in cleartext. golemctl attaches it anyway — an operator with a segmented
//! network or a VPN may mean exactly that — but warns on stderr each time, so
//! the choice is never made silently ([`is_loopback_base`] decides).
//!
//! [`AuthSource`] and [`Conn`] both hand-write `Debug` to print [`REDACTED`] in
//! place of the secret. Every derived `Debug` in golemctl is one `{:?}` in an
//! error path away from putting the token in a log or a terminal, so the type
//! that holds it never learns to print it.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use reqwest::{header::AUTHORIZATION, StatusCode};

use crate::inventory::{Endpoint, Target};
use crate::poll::{Progress, Reconcile202};
use crate::tunnel::{ssh_bin, Tunnel};

pub const AUTH_TOKEN_ENV: &str = "GOLEM_AUTH_TOKEN";
pub const AUTH_TOKEN_FILE_ENV: &str = "GOLEM_AUTH_TOKEN_FILE";

/// The ambient credential a run carries, resolved once before any host is
/// contacted so a fan-out reads the environment a single time. `None` means
/// requests go out with no `Authorization` header.
#[derive(Clone, PartialEq, Eq)]
pub enum AuthSource {
    None,
    Token(String),
}

pub const REDACTED: &str = "<redacted>";

impl std::fmt::Debug for AuthSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthSource::None => write!(f, "None"),
            AuthSource::Token(_) => write!(f, "Token({REDACTED})"),
        }
    }
}

impl AuthSource {
    fn token(&self) -> Option<String> {
        match self {
            AuthSource::None => None,
            AuthSource::Token(token) => Some(token.clone()),
        }
    }
}

/// The token to use, taking the first of: a host's own inventory `token_file`,
/// `GOLEM_AUTH_TOKEN`, the file at `GOLEM_AUTH_TOKEN_FILE`. Absent everywhere
/// is [`AuthSource::None`], not an error — golemctl still talks to ungated
/// daemons.
pub fn resolve_auth(inventory_token_file: Option<&Path>) -> Result<AuthSource> {
    if let Some(path) = inventory_token_file {
        return Ok(AuthSource::Token(read_token_file(path)?));
    }
    if let Some(token) = env_value(AUTH_TOKEN_ENV) {
        return Ok(AuthSource::Token(token));
    }
    if let Some(path) = env_path(AUTH_TOKEN_FILE_ENV) {
        return Ok(AuthSource::Token(read_token_file(&path)?));
    }
    Ok(AuthSource::None)
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

fn read_token_file(path: &Path) -> Result<String> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("read auth token file {}", path.display()))?;
    let trimmed = contents.trim_end();
    if trimmed.is_empty() {
        return Err(anyhow!("auth token file {} is empty", path.display()));
    }
    Ok(trimmed.to_string())
}

const LOCALHOST_NAME: &str = "localhost";

/// Whether a base URL addresses this machine — `127.0.0.0/8`, `::1`, or
/// `localhost`. Anything golemctl cannot read as one of those counts as remote,
/// so an unparseable address warns rather than passes.
pub fn is_loopback_base(base: &str) -> bool {
    let Some(host) = host_of(base) else {
        return false;
    };
    host.eq_ignore_ascii_case(LOCALHOST_NAME)
        || host.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
        || host.parse::<Ipv6Addr>().is_ok_and(|ip| ip.is_loopback())
}

fn host_of(base: &str) -> Option<&str> {
    let after_scheme = base.split_once("://").map_or(base, |(_, rest)| rest);
    let authority = after_scheme.split(['/', '?', '#']).next()?;
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if let Some(bracketed) = authority.strip_prefix('[') {
        return bracketed.split_once(']').map(|(host, _)| host);
    }
    authority.split(':').next().filter(|host| !host.is_empty())
}

fn cleartext_token_warning(name: &str, base: &str) -> String {
    format!(
        "warning: host {name} is dialed at {base}, which is not loopback — the auth token crosses that network in cleartext; use an ssh:// target to keep it inside the tunnel (ADR 0042)"
    )
}

/// Open the forward off the async runtime: [`Tunnel::open`] spawns ssh and then
/// polls a socket until it answers, which would block a runtime worker — and
/// with a fleet fan-out, every other host's task with it.
async fn forward(
    destination: String,
    ssh_port: Option<u16>,
    remote_port: u16,
    ssh_args: Vec<String>,
) -> Result<Tunnel> {
    tokio::task::spawn_blocking(move || {
        Tunnel::open(&destination, ssh_port, remote_port, &ssh_args, &ssh_bin())
    })
    .await
    .context("open an ssh forward")?
}

/// An open line to one daemon: where to send, what to send with it, and — for
/// an ssh target — the forward that line rides on, kept alive exactly as long
/// as the `Conn` is.
///
/// NOTE: field order is load-bearing. Rust drops fields in declaration order,
/// so `client` (and the keep-alive sockets in its pool) must be declared before
/// `tunnel`; reversing them kills ssh while connections through it are still
/// open.
pub struct Conn {
    base: String,
    token: Option<String>,
    client: reqwest::Client,
    tunnel: Option<Tunnel>,
}

impl std::fmt::Debug for Conn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conn")
            .field("base", &self.base)
            .field("token", &self.token.as_ref().map(|_| REDACTED))
            .field("tunnel", &self.tunnel)
            .finish()
    }
}

impl Conn {
    pub async fn open(target: &Target, auth: &AuthSource) -> Result<Conn> {
        let (base, tunnel, reached_over_ssh) = match &target.endpoint {
            Endpoint::Http { url } => (url.trim_end_matches('/').to_string(), None, false),
            Endpoint::Ssh {
                destination,
                ssh_port,
                remote_port,
                ssh_args,
            } => {
                let mut tunnel = forward(
                    destination.clone(),
                    *ssh_port,
                    *remote_port,
                    ssh_args.clone(),
                )
                .await?;
                tunnel.confirm_alive()?;
                (
                    format!("http://127.0.0.1:{}", tunnel.local_port),
                    Some(tunnel),
                    true,
                )
            }
        };
        let token = match &target.token_file {
            Some(path) => resolve_auth(Some(path))?.token(),
            None => auth.token(),
        };
        if token.is_some() && !reached_over_ssh && !is_loopback_base(&base) {
            eprintln!("{}", cleartext_token_warning(&target.name, &base));
        }
        Ok(Conn {
            base,
            token,
            client: reqwest::Client::new(),
            tunnel,
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base, path.trim_start_matches('/'))
    }

    fn authorize(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.token {
            Some(token) => builder.header(AUTHORIZATION, format!("Bearer {token}")),
            None => builder,
        }
    }

    pub async fn get_json(&self, path: &str) -> Result<serde_json::Value> {
        let url = self.url(path);
        let resp = self
            .authorize(self.client.get(&url))
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;
        let text = expect_success(resp).await?;
        serde_json::from_str(&text).with_context(|| format!("decode {url}"))
    }

    pub async fn post_bytes(&self, path: &str, bytes: Vec<u8>) -> Result<reqwest::Response> {
        let url = self.url(path);
        self.authorize(
            self.client
                .post(&url)
                .header("content-type", "application/octet-stream"),
        )
        .body(bytes)
        .send()
        .await
        .with_context(|| format!("POST {url}"))
    }

    pub async fn post_manifest(&self, bytes: Vec<u8>) -> Result<Reconcile202> {
        let resp = self.post_bytes("manifest", bytes).await?;
        let text = expect_status(resp, StatusCode::ACCEPTED).await?;
        Ok(serde_json::from_str(&text)?)
    }

    pub async fn post_plan(&self, bytes: Vec<u8>) -> Result<String> {
        let resp = self.post_bytes("plan", bytes).await?;
        expect_success(resp).await
    }

    pub async fn get_progress(&self, id: u64, after: u64) -> Result<Progress> {
        let value = self
            .get_json(&format!("reconciles/{id}?after={after}"))
            .await?;
        Ok(serde_json::from_value(value)?)
    }

    pub async fn get_latest(&self, after: u64) -> Result<Progress> {
        let value = self
            .get_json(&format!("reconciles/latest?after={after}"))
            .await?;
        Ok(serde_json::from_value(value)?)
    }
}

async fn expect_status(resp: reqwest::Response, expected: StatusCode) -> Result<String> {
    let status = resp.status();
    let text = resp.text().await?;
    if status == StatusCode::UNAUTHORIZED {
        return Err(unauthorized_error());
    }
    if status != expected {
        return Err(response_error(status, &text));
    }
    Ok(text)
}

async fn expect_success(resp: reqwest::Response) -> Result<String> {
    let status = resp.status();
    let text = resp.text().await?;
    if status == StatusCode::UNAUTHORIZED {
        return Err(unauthorized_error());
    }
    if !status.is_success() {
        return Err(response_error(status, &text));
    }
    Ok(text)
}

fn response_error(status: StatusCode, text: &str) -> anyhow::Error {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
            return anyhow!("{status}: {msg}");
        }
    }
    anyhow!("{status}: {text}")
}

/// A `401` says the daemon is gated and this run holds the wrong secret or
/// none. The message names every place golemctl would have looked, because the
/// fix is always to put the secret in one of them — not to retry.
fn unauthorized_error() -> anyhow::Error {
    anyhow!(
        "unauthorized — set {AUTH_TOKEN_ENV} or {AUTH_TOKEN_FILE_ENV} (or an inventory host's token_file) to golemd's configured secret"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, MutexGuard};

    // The precedence tests set process-wide environment variables, so they hold
    // this for their whole body and restore what was there on the way out.
    // Cargo runs a crate's tests on threads of one process; without the lock
    // one test's `GOLEM_AUTH_TOKEN` decides another's outcome.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    struct EnvGuard {
        token: Option<std::ffi::OsString>,
        token_file: Option<std::ffi::OsString>,
        _lock: MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn set(token: Option<&str>, token_file: Option<&Path>) -> Self {
            let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev_token = std::env::var_os(AUTH_TOKEN_ENV);
            let prev_token_file = std::env::var_os(AUTH_TOKEN_FILE_ENV);
            set_or_clear(AUTH_TOKEN_ENV, token.map(std::ffi::OsStr::new));
            set_or_clear(AUTH_TOKEN_FILE_ENV, token_file.map(Path::as_os_str));
            EnvGuard {
                token: prev_token,
                token_file: prev_token_file,
                _lock: lock,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            set_or_clear(AUTH_TOKEN_ENV, self.token.as_deref());
            set_or_clear(AUTH_TOKEN_FILE_ENV, self.token_file.as_deref());
        }
    }

    fn set_or_clear(name: &str, value: Option<&std::ffi::OsStr>) {
        match value {
            Some(v) => std::env::set_var(name, v),
            None => std::env::remove_var(name),
        }
    }

    #[test]
    fn absent_everywhere_resolves_to_none() {
        let _env = EnvGuard::set(None, None);
        assert_eq!(resolve_auth(None).unwrap(), AuthSource::None);
    }

    #[test]
    fn the_env_token_wins_over_the_env_token_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "file-token\n").unwrap();
        let _env = EnvGuard::set(Some("env-token"), Some(&path));
        assert_eq!(
            resolve_auth(None).unwrap(),
            AuthSource::Token("env-token".to_string())
        );
    }

    #[test]
    fn the_env_token_file_is_read_and_trimmed_when_no_direct_token_is_set() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "file-token\n").unwrap();
        let _env = EnvGuard::set(None, Some(&path));
        assert_eq!(
            resolve_auth(None).unwrap(),
            AuthSource::Token("file-token".to_string())
        );
    }

    #[test]
    fn the_inventory_token_file_wins_over_the_environment() {
        let dir = tempfile::tempdir().unwrap();
        let inventory_path = dir.path().join("inventory-token");
        std::fs::write(&inventory_path, "inventory-token").unwrap();
        let env_file_path = dir.path().join("env-token-file");
        std::fs::write(&env_file_path, "env-file-token").unwrap();
        let _env = EnvGuard::set(Some("env-token"), Some(&env_file_path));
        assert_eq!(
            resolve_auth(Some(&inventory_path)).unwrap(),
            AuthSource::Token("inventory-token".to_string())
        );
    }

    #[test]
    fn an_empty_env_token_file_after_trim_is_a_startup_error_naming_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "\n").unwrap();
        let _env = EnvGuard::set(None, Some(&path));
        let err = resolve_auth(None).unwrap_err();
        assert!(err.to_string().contains(&path.display().to_string()));
    }

    #[test]
    fn loopback_bases_are_the_only_ones_a_token_may_cross_in_the_clear() {
        for base in [
            "http://127.0.0.1:8807",
            "http://127.1.2.3:8807",
            "http://localhost:8807",
            "http://LocalHost",
            "http://[::1]:7474",
            "https://127.0.0.1",
            "http://golem@127.0.0.1:8807",
            "http://127.0.0.1:8807/",
        ] {
            assert!(is_loopback_base(base), "{base}");
        }
        for base in [
            "http://10.0.0.5:8807",
            "http://scaly:8807",
            "http://scaly",
            "https://golem.example.org",
            "http://[2001:db8::1]:7474",
            "http://127.0.0.1.example.org:8807",
            "http://",
            "",
        ] {
            assert!(!is_loopback_base(base), "{base}");
        }
    }

    #[test]
    fn the_cleartext_warning_names_the_host_the_address_and_the_safe_alternative() {
        let warning = cleartext_token_warning("scaly", "http://10.0.0.5:8807");
        assert!(!warning.contains('\n'), "{warning}");
        assert!(warning.contains("scaly"), "{warning}");
        assert!(warning.contains("http://10.0.0.5:8807"), "{warning}");
        assert!(warning.contains("ssh://"), "{warning}");
    }

    fn http_target(name: &str, url: String) -> Target {
        Target {
            name: name.into(),
            endpoint: Endpoint::Http { url },
            token_file: None,
        }
    }

    #[test]
    fn neither_a_conn_nor_an_auth_source_prints_the_token_it_carries() {
        assert_eq!(
            format!("{:?}", AuthSource::Token("secret".into())),
            "Token(<redacted>)"
        );
        assert_eq!(format!("{:?}", AuthSource::None), "None");
        let conn = Conn {
            base: "http://scaly:8807".into(),
            token: Some("secret".into()),
            client: reqwest::Client::new(),
            tunnel: None,
        };
        let shown = format!("{conn:?}");
        assert!(!shown.contains("secret"), "{shown}");
        assert!(shown.contains(REDACTED), "{shown}");
    }

    #[tokio::test]
    async fn a_hosts_own_token_file_authorizes_it_over_the_ambient_auth() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token");
        std::fs::write(&path, "secret\n").unwrap();
        let base = serve_gated("h1", "secret").await;
        let target = Target {
            name: "h1".into(),
            endpoint: Endpoint::Http { url: base },
            token_file: Some(path),
        };
        let conn = Conn::open(&target, &AuthSource::Token("wrong".into()))
            .await
            .unwrap();
        assert_eq!(conn.get_json("status").await.unwrap()["host"], "h1");
    }

    async fn serve_gated(name: &str, token: &str) -> String {
        let foreman = golemd::foreman::Foreman::new(
            name.to_string(),
            Box::new(golemd::planroom::MemoryPlanRoom::new()),
            Box::new(golemd::fake_reconciler::FakeReconciler::new()),
        );
        let app = golemd::http::router(golemd::http::AppState {
            foreman: Arc::new(foreman),
            required_token: Some(Arc::new(token.to_string())),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn a_conn_carrying_the_right_token_reaches_a_gated_daemon() {
        let base = serve_gated("h1", "secret").await;
        let target = http_target("h1", base);
        let conn = Conn::open(&target, &AuthSource::Token("secret".into()))
            .await
            .unwrap();
        let status = conn.get_json("status").await.unwrap();
        assert_eq!(status["host"], "h1");
    }

    #[tokio::test]
    async fn a_401_error_names_the_env_vars_an_operator_can_set() {
        let base = serve_gated("h1", "secret").await;
        let target = http_target("h1", base);
        let conn = Conn::open(&target, &AuthSource::None).await.unwrap();
        let err = conn.get_json("status").await.unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains(AUTH_TOKEN_ENV), "{message}");
        assert!(message.contains(AUTH_TOKEN_FILE_ENV), "{message}");
    }
}
