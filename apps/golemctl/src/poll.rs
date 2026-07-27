//! The typed client for golemd's fire-then-poll apply protocol (ADR 0033
//! §1–2). [`post_manifest`] fires the manifest and gets back a `202
//! { reconcile_id }`; [`get_progress`] then polls `GET
//! /reconciles/<id>?after=<cursor>` until the projection settles. The
//! projection — the folded per-glyph [`GlyphState`] under each [`UnitProgress`]
//! — is the truth; `events` is the ordered log golemd streams alongside it,
//! rendered as garnish under the active unit. [`get_latest`] hits
//! `/reconciles/latest` to reattach to the newest attempt when the caller has
//! lost its id.
//!
//! These types mirror golemd's `report`/projection wire shape; `serde` field
//! and variant names are the contract, so their spelling matches the JSON
//! exactly (`snake_case` enums).

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
    // A shared duplicate settled by crediting another unit's success rather than
    // by real work (ADR 0034 §1) — rendered with the `≡` mark, never a bright ✓.
    Credited,
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
    // NOTE: server-computed and clamped, read from golemd's live round loop —
    // the WAL never records a scheduled retry (ADR 0033 §2). Present only while
    // a retry is pending; absent on a recovered attempt after a daemon restart.
    pub next_retry_in_ms: Option<u64>,
    // Additive dedup facts (ADR 0034 §1): `shared` marks a duplicate an earlier
    // unit already enacts, `owner` names that first declarer's unit_path. A
    // pre-dedup golemd omits both, so they default tolerantly.
    #[serde(default)]
    pub shared: bool,
    #[serde(default)]
    pub owner: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UnitProgress {
    pub unit_path: Vec<String>,
    pub glyphs: Vec<GlyphProgress>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    #[default]
    Lifecycle,
    Cmd,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub at: String,
    pub level: String,
    // Additive (ADR 0033 §2): a record with no `kind` reads as `lifecycle`, so a
    // pre-`kind` golemd's events still parse.
    #[serde(default)]
    pub kind: EventKind,
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
    // The next `?after` to pass: events past this are unseen. Resuming from it
    // after a dropped poll misses nothing the server buffer still holds.
    pub cursor: u64,
    // `null` until the attempt settles, then the full `ReconcileReport` (ADR
    // 0029 §5) — the same body the synchronous 200 used to return. Kept as raw
    // JSON: golemctl only reads `outcome` for its exit code and pretty-prints
    // the rest.
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
    let url = format!(
        "{}/reconciles/{id}?after={after}",
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
    fn an_event_without_kind_defaults_to_lifecycle_and_cmd_parses() {
        let old = serde_json::json!({
            "seq": 1, "at": "t", "level": "info",
            "unit_path": ["h"], "glyph_key": "apt:x", "message": "install apt:x"
        });
        let ev: Event = serde_json::from_value(old).unwrap();
        assert_eq!(ev.kind, EventKind::Lifecycle);

        let cmd = serde_json::json!({
            "seq": 2, "at": "t", "level": "info", "kind": "cmd",
            "unit_path": ["h"], "glyph_key": "apt:x", "message": "Unpacking x ..."
        });
        let ev: Event = serde_json::from_value(cmd).unwrap();
        assert_eq!(ev.kind, EventKind::Cmd);
    }

    #[test]
    fn a_glyph_without_dedup_fields_defaults_to_not_shared() {
        let old = serde_json::json!({
            "glyph_key": "apt:podman", "action": "install",
            "state": "applied", "rounds": 1, "next_retry_in_ms": null
        });
        let g: GlyphProgress = serde_json::from_value(old).unwrap();
        assert!(!g.shared);
        assert!(g.owner.is_none());
    }

    #[test]
    fn a_shared_glyph_carries_its_owner_and_credited_state() {
        let shared = serde_json::json!({
            "glyph_key": "apt:podman", "action": "install",
            "state": "credited", "rounds": 1, "next_retry_in_ms": null,
            "shared": true, "owner": ["scaly", "first"]
        });
        let g: GlyphProgress = serde_json::from_value(shared).unwrap();
        assert!(g.shared);
        assert_eq!(
            g.owner,
            Some(vec!["scaly".to_string(), "first".to_string()])
        );
        assert_eq!(g.state, GlyphState::Credited);
    }

    #[test]
    fn a_settled_phase_is_terminal() {
        assert!(Phase::Settled.is_terminal());
        assert!(Phase::RolledBack.is_terminal());
        assert!(!Phase::Planning.is_terminal());
    }
}
