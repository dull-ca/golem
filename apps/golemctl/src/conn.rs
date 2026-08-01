use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use reqwest::{header::AUTHORIZATION, StatusCode};

use crate::inventory::Target;
use crate::poll::{Progress, Reconcile202};

pub const AUTH_TOKEN_ENV: &str = "GOLEM_AUTH_TOKEN";
pub const AUTH_TOKEN_FILE_ENV: &str = "GOLEM_AUTH_TOKEN_FILE";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthSource {
    None,
    Token(String),
}

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

#[derive(Debug, Clone)]
pub struct Conn {
    base: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl Conn {
    pub async fn open(target: &Target, auth: &AuthSource) -> Result<Conn> {
        Ok(Conn {
            base: target.addr.trim_end_matches('/').to_string(),
            token: match auth {
                AuthSource::None => None,
                AuthSource::Token(token) => Some(token.clone()),
            },
            client: reqwest::Client::new(),
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

fn unauthorized_error() -> anyhow::Error {
    anyhow!(
        "unauthorized — set {AUTH_TOKEN_ENV} or {AUTH_TOKEN_FILE_ENV} (or an inventory host's token_file) to golemd's configured secret"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex, MutexGuard};

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
        let target = Target {
            name: "h1".into(),
            addr: base,
        };
        let conn = Conn::open(&target, &AuthSource::Token("secret".into()))
            .await
            .unwrap();
        let status = conn.get_json("status").await.unwrap();
        assert_eq!(status["host"], "h1");
    }

    #[tokio::test]
    async fn a_401_error_names_the_env_vars_an_operator_can_set() {
        let base = serve_gated("h1", "secret").await;
        let target = Target {
            name: "h1".into(),
            addr: base,
        };
        let conn = Conn::open(&target, &AuthSource::None).await.unwrap();
        let err = conn.get_json("status").await.unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains(AUTH_TOKEN_ENV), "{message}");
        assert!(message.contains(AUTH_TOKEN_FILE_ENV), "{message}");
    }
}
