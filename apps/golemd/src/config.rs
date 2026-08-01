//! golemd's operational config: the `golemd.toml` `[retry]` and `[enact]` tables.
//!
//! This is golemd's PRIVATE surface — it is never on the manifest wire and never
//! hashed. Absent file, every field falls back to a built-in default, so a
//! CLI-only invocation keeps working. The path is named by `--config`
//! (`main.rs`); no file means `load(None)`.
//!
//! Every field is optional. `RetryConfig::default()` IS the built-in defaults:
//! 200ms base delay, 2.0 backoff, 30s max delay, 0.2 jitter, 5 attempts, 120s
//! wall budget, rollback on exhaust. A field present in the file overrides its
//! default; an absent field keeps it.
//!
//! ```toml
//! # golemd.toml — all fields optional; shown values are the built-in defaults.
//! [retry]
//! base_delay_ms      = 200      # delay before the first retry round
//! backoff_multiplier = 2.0      # each round multiplies the prior delay
//! max_delay_ms       = 30000    # ceiling on the per-round delay
//! jitter_fraction    = 0.2      # ± this fraction of the delay, uniform
//! max_attempts       = 5        # cap on rounds per op (round 1 + up to 4 retries)
//! max_elapsed_ms     = 120000   # wall-time budget for the whole reconcile's retrying
//! on_exhaust         = "rollback"  # "rollback" | "keep" when a limit trips
//!
//! [enact]
//! workers            = 4        # concurrent units the parallel enact executor runs
//! ```
//!
//! The `[retry]` table is the fleet-wide default; the per-scroll `policy`
//! cascade overrides it, nearest scope winning (`foreman::resolve_retry`,
//! ADR 0029 §3, ADR 0031 §3). `[enact]` has no per-scroll override — it is a
//! host-wide knob.

use std::path::{Path, PathBuf};

use scroll_format::OnExhaust;
use serde::Deserialize;

/// What a leaf unit does when a retry limit trips with glyphs still failing:
/// `Rollback` undoes this attempt's applied glyphs for that unit; `Keep` leaves
/// them (ADR 0029 §4). The TOML spelling is lowercase (`"rollback"` | `"keep"`).
/// Mirrors `scroll_format::OnExhaust`, kept separate because this is the config
/// side, not the wire side.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OnExhaustConfig {
    Rollback,
    Keep,
}

impl OnExhaustConfig {
    pub fn to_on_exhaust(self) -> OnExhaust {
        match self {
            OnExhaustConfig::Rollback => OnExhaust::Rollback,
            OnExhaustConfig::Keep => OnExhaust::Keep,
        }
    }
}

/// The retry pace and exhaustion behavior for one leaf unit's enact.
///
/// The per-round delay is
/// `min(max_delay_ms, base_delay_ms × backoff_multiplier^(round-1))`, then
/// perturbed by ± `jitter_fraction` (uniform). Jitter de-synchronizes retries
/// across a fleet: without it, N hosts that failed the same upstream in lockstep
/// would hammer it in lockstep.
///
/// `max_attempts` (rounds per op) and `max_elapsed_ms` (total retrying
/// wall-time) are dual limits — whichever trips first ends retrying. `on_exhaust`
/// then selects rollback or keep for the failing unit's subtree (ADR 0029 §4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RetryConfig {
    pub base_delay_ms: u64,
    pub backoff_multiplier: f64,
    pub max_delay_ms: u64,
    pub jitter_fraction: f64,
    pub max_attempts: u32,
    pub max_elapsed_ms: u64,
    pub on_exhaust: OnExhaustConfig,
}

impl Default for RetryConfig {
    fn default() -> Self {
        RetryConfig {
            base_delay_ms: 200,
            backoff_multiplier: 2.0,
            max_delay_ms: 30_000,
            jitter_fraction: 0.2,
            max_attempts: 5,
            max_elapsed_ms: 120_000,
            on_exhaust: OnExhaustConfig::Rollback,
        }
    }
}

/// How many leaf units the enact executor runs concurrently. Consumed by the
/// coming parallel-unit executor (ADR 0034 §3), which the attempt-scoped claim
/// and success sets are already Mutex-guarded for; the units loop is serial
/// until it lands, so today this only sets the width it will use. `workers = 1`
/// is the serial fallback.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnactConfig {
    pub workers: usize,
}

impl Default for EnactConfig {
    fn default() -> Self {
        EnactConfig { workers: 4 }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuthConfig {
    pub token_file: Option<PathBuf>,
}

/// The whole resolved `golemd.toml`: the fleet-default retry pace and the
/// host-wide enact width.
#[derive(Debug, Clone, PartialEq)]
pub struct GolemdConfig {
    pub retry: RetryConfig,
    pub enact: EnactConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Default, Deserialize)]
struct FileShape {
    retry: Option<RetryTable>,
    enact: Option<EnactTable>,
    auth: Option<AuthTable>,
}

#[derive(Debug, Default, Deserialize)]
struct EnactTable {
    workers: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct AuthTable {
    token_file: Option<PathBuf>,
}

#[derive(Debug, Default, Deserialize)]
struct RetryTable {
    base_delay_ms: Option<u64>,
    backoff_multiplier: Option<f64>,
    max_delay_ms: Option<u64>,
    jitter_fraction: Option<f64>,
    max_attempts: Option<u32>,
    max_elapsed_ms: Option<u64>,
    on_exhaust: Option<OnExhaustConfig>,
}

/// A `--config` path was given but could not be read (`Read`) or was not valid
/// TOML for the `[retry]` shape (`Parse`). Absent path is not an error — it
/// yields the defaults.
#[derive(Debug)]
pub enum ConfigError {
    Read(String),
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Read(m) => write!(f, "could not read golemd config: {m}"),
            ConfigError::Parse(m) => write!(f, "could not parse golemd config: {m}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Resolve the fleet-default `RetryConfig`. `None` (no `--config`) is the
/// defaults; a path is read and its present `[retry]` fields override the
/// defaults field by field, absent fields keeping them.
pub fn load(path: Option<&Path>) -> Result<GolemdConfig, ConfigError> {
    let Some(path) = path else {
        return Ok(GolemdConfig {
            retry: RetryConfig::default(),
            enact: EnactConfig::default(),
            auth: AuthConfig::default(),
        });
    };
    let text = std::fs::read_to_string(path).map_err(|e| ConfigError::Read(e.to_string()))?;
    let shape: FileShape = toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
    let mut retry = RetryConfig::default();
    if let Some(t) = shape.retry {
        if let Some(v) = t.base_delay_ms {
            retry.base_delay_ms = v;
        }
        if let Some(v) = t.backoff_multiplier {
            retry.backoff_multiplier = v;
        }
        if let Some(v) = t.max_delay_ms {
            retry.max_delay_ms = v;
        }
        if let Some(v) = t.jitter_fraction {
            retry.jitter_fraction = v;
        }
        if let Some(v) = t.max_attempts {
            retry.max_attempts = v;
        }
        if let Some(v) = t.max_elapsed_ms {
            retry.max_elapsed_ms = v;
        }
        if let Some(v) = t.on_exhaust {
            retry.on_exhaust = v;
        }
    }
    let mut enact = EnactConfig::default();
    if let Some(t) = shape.enact {
        if let Some(w) = t.workers {
            enact.workers = w;
        }
    }
    let mut auth = AuthConfig::default();
    if let Some(t) = shape.auth {
        if let Some(f) = t.token_file {
            auth.token_file = Some(f);
        }
    }
    Ok(GolemdConfig { retry, enact, auth })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_path_gives_builtin_defaults() {
        let cfg = load(None).unwrap();
        assert_eq!(cfg.retry, RetryConfig::default());
        assert_eq!(cfg.retry.max_attempts, 5);
        assert_eq!(cfg.retry.on_exhaust, OnExhaustConfig::Rollback);
        assert_eq!(cfg.auth.token_file, None);
    }

    #[test]
    fn auth_token_file_parses() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golemd.toml");
        std::fs::write(&path, "[auth]\ntoken_file = \"/etc/golem/token\"\n").unwrap();
        let cfg = load(Some(&path)).unwrap();
        assert_eq!(cfg.auth.token_file, Some(PathBuf::from("/etc/golem/token")));
    }

    #[test]
    fn enact_workers_defaults_to_four_and_overrides() {
        let cfg = load(None).unwrap();
        assert_eq!(cfg.enact.workers, 4, "default worker count is 4");

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golemd.toml");
        std::fs::write(&path, "[enact]\nworkers = 1\n").unwrap();
        let cfg = load(Some(&path)).unwrap();
        assert_eq!(cfg.enact.workers, 1, "workers = 1 is the serial fallback");
        assert_eq!(
            cfg.retry.max_attempts, 5,
            "an [enact]-only file keeps retry defaults"
        );
    }

    #[test]
    fn present_fields_override_defaults_absent_fields_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golemd.toml");
        std::fs::write(&path, "[retry]\nmax_attempts = 9\non_exhaust = \"keep\"\n").unwrap();
        let cfg = load(Some(&path)).unwrap();
        assert_eq!(cfg.retry.max_attempts, 9);
        assert_eq!(cfg.retry.on_exhaust, OnExhaustConfig::Keep);
        assert_eq!(cfg.retry.base_delay_ms, 200);
        assert_eq!(cfg.retry.backoff_multiplier, 2.0);
    }

    #[test]
    fn a_malformed_config_is_a_typed_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golemd.toml");
        std::fs::write(&path, "[retry]\nmax_attempts = \"lots\"\n").unwrap();
        match load(Some(&path)) {
            Err(ConfigError::Parse(_)) => {}
            other => panic!("expected Parse, got {other:?}"),
        }
    }
}
