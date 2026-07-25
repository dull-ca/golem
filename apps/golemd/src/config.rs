use std::path::Path;

use scroll_format::OnExhaust;
use serde::Deserialize;

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

#[derive(Debug, Default, Deserialize)]
struct FileShape {
    retry: Option<RetryTable>,
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

pub fn load(path: Option<&Path>) -> Result<RetryConfig, ConfigError> {
    let Some(path) = path else {
        return Ok(RetryConfig::default());
    };
    let text = std::fs::read_to_string(path).map_err(|e| ConfigError::Read(e.to_string()))?;
    let shape: FileShape = toml::from_str(&text).map_err(|e| ConfigError::Parse(e.to_string()))?;
    let mut cfg = RetryConfig::default();
    if let Some(t) = shape.retry {
        if let Some(v) = t.base_delay_ms {
            cfg.base_delay_ms = v;
        }
        if let Some(v) = t.backoff_multiplier {
            cfg.backoff_multiplier = v;
        }
        if let Some(v) = t.max_delay_ms {
            cfg.max_delay_ms = v;
        }
        if let Some(v) = t.jitter_fraction {
            cfg.jitter_fraction = v;
        }
        if let Some(v) = t.max_attempts {
            cfg.max_attempts = v;
        }
        if let Some(v) = t.max_elapsed_ms {
            cfg.max_elapsed_ms = v;
        }
        if let Some(v) = t.on_exhaust {
            cfg.on_exhaust = v;
        }
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_path_gives_builtin_defaults() {
        let cfg = load(None).unwrap();
        assert_eq!(cfg, RetryConfig::default());
        assert_eq!(cfg.max_attempts, 5);
        assert_eq!(cfg.on_exhaust, OnExhaustConfig::Rollback);
    }

    #[test]
    fn present_fields_override_defaults_absent_fields_do_not() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("golemd.toml");
        std::fs::write(&path, "[retry]\nmax_attempts = 9\non_exhaust = \"keep\"\n").unwrap();
        let cfg = load(Some(&path)).unwrap();
        assert_eq!(cfg.max_attempts, 9);
        assert_eq!(cfg.on_exhaust, OnExhaustConfig::Keep);
        assert_eq!(cfg.base_delay_ms, 200);
        assert_eq!(cfg.backoff_multiplier, 2.0);
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
