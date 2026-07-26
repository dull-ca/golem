use anyhow::{bail, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Reconcile202 {
    pub reconcile_id: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlyphState {
    Pending,
    InProgress,
    Applied,
    Unchanged,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Planning,
    Enacting,
    Settling,
    Settled,
    RolledBack,
}

impl Phase {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Phase::Settled | Phase::RolledBack)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlyphProgress {
    pub glyph_key: String,
    pub action: String,
    pub state: GlyphState,
    pub rounds: u32,
    pub next_retry_in_ms: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitProgress {
    pub unit_path: Vec<String>,
    pub glyphs: Vec<GlyphProgress>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub at: String,
    pub level: String,
    pub unit_path: Vec<String>,
    pub glyph_key: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Progress {
    pub reconcile_id: u64,
    pub phase: Phase,
    pub units: Vec<UnitProgress>,
    pub events: Vec<Event>,
    pub cursor: u64,
    pub report: Option<serde_json::Value>,
}

pub async fn post_manifest(addr: &str, bytes: Vec<u8>) -> Result<Reconcile202> {
    let url = format!("{}/manifest", addr.trim_end_matches('/'));
    let resp = reqwest::Client::new()
        .post(&url)
        .header("content-type", "application/octet-stream")
        .body(bytes)
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    if status.as_u16() != 202 {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(msg) = v.get("message").and_then(|m| m.as_str()) {
                bail!("{status}: {msg}");
            }
        }
        bail!("{status}: {text}");
    }
    Ok(serde_json::from_str(&text)?)
}

pub async fn get_progress(addr: &str, id: u64, after: u64) -> Result<Progress> {
    let url = format!("{}/reconciles/{id}?after={after}", addr.trim_end_matches('/'));
    let resp = reqwest::get(&url).await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        bail!("{status}: {text}");
    }
    Ok(serde_json::from_str(&text)?)
}

pub async fn get_latest(addr: &str, after: u64) -> Result<Progress> {
    let url = format!(
        "{}/reconciles/latest?after={after}",
        addr.trim_end_matches('/')
    );
    let resp = reqwest::get(&url).await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        bail!("{status}: {text}");
    }
    Ok(serde_json::from_str(&text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_progress_payload_deserializes() {
        let json = serde_json::json!({
            "reconcile_id": 42,
            "phase": "enacting",
            "units": [
                { "unit_path": ["scaly","a"],
                  "glyphs": [
                    { "glyph_key": "apt:podman", "action": "install",
                      "state": "in_progress", "rounds": 1, "next_retry_in_ms": null }
                  ] }
            ],
            "events": [
                { "seq": 18, "at": "2026-07-26T00:00:00Z", "level": "info",
                  "unit_path": ["scaly","a"], "glyph_key": "apt:podman",
                  "message": "install apt:podman" }
            ],
            "cursor": 18,
            "report": null
        });
        let p: Progress = serde_json::from_value(json).unwrap();
        assert_eq!(p.reconcile_id, 42);
        assert!(matches!(p.phase, Phase::Enacting));
        assert!(!p.phase.is_terminal());
        assert_eq!(p.units[0].glyphs[0].glyph_key, "apt:podman");
        assert!(matches!(p.units[0].glyphs[0].state, GlyphState::InProgress));
        assert_eq!(p.cursor, 18);
        assert!(p.report.is_none());
    }

    #[test]
    fn a_settled_phase_is_terminal() {
        assert!(Phase::Settled.is_terminal());
        assert!(Phase::RolledBack.is_terminal());
        assert!(!Phase::Planning.is_terminal());
    }
}
