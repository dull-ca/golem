//! The in-memory half of the progress stream (ADR 0033 §2): the two facts the
//! WAL cannot carry — a round's failure *reason*/*delay* line, and the live
//! retry countdown — held per running attempt. The projection (`projection.rs`)
//! folds the durable per-glyph *states* from the WAL and layers these on top.
//!
//! Everything here is **lost on daemon restart**, by design. Recovery re-drives
//! the WAL and reconstructs the states in full, so a reattaching client always
//! sees correct states and resumes the event stream from the recovered
//! attempt's WAL-derived events — only the transient pre-crash round lines are
//! gone (ADR 0033 §2, "states are durable, the finest log lines are
//! best-effort").
//!
//! `seq` is a per-attempt monotone cursor: `record` stamps each event with the
//! next `seq`, and `events_after(after)` returns the slice `> after`, so a
//! client passes back the `cursor` it last saw and misses nothing the ring
//! still holds. The ring is bounded at `EVENT_RING_CAP` events per attempt
//! (oldest dropped past the cap, `seq` stays monotone across the drop) and only
//! `ATTEMPT_LRU` attempts are kept live at once — a poll targets the current or
//! just-finished attempt, so two is enough and a chatty rollback cannot bloat
//! daemon memory unbounded.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::Serialize;

pub const EVENT_RING_CAP: usize = 1024;
const ATTEMPT_LRU: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub level: EventLevel,
    pub unit_path: Vec<String>,
    pub glyph_key: String,
    pub message: String,
}

struct AttemptRing {
    next_seq: u64,
    events: VecDeque<ProgressEvent>,
    retries: BTreeMap<String, u64>,
}

impl AttemptRing {
    fn new() -> Self {
        Self {
            next_seq: 1,
            events: VecDeque::new(),
            retries: BTreeMap::new(),
        }
    }
}

struct Inner {
    rings: BTreeMap<u64, AttemptRing>,
    order: VecDeque<u64>,
}

pub struct ProgressRegistry {
    inner: Mutex<Inner>,
}

impl ProgressRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                rings: BTreeMap::new(),
                order: VecDeque::new(),
            }),
        }
    }

    pub fn open(&self, reconcile_id: u64) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.rings.contains_key(&reconcile_id) {
            inner.rings.insert(reconcile_id, AttemptRing::new());
            inner.order.push_back(reconcile_id);
            while inner.order.len() > ATTEMPT_LRU {
                if let Some(evicted) = inner.order.pop_front() {
                    inner.rings.remove(&evicted);
                }
            }
        }
    }

    pub fn record(
        &self,
        reconcile_id: u64,
        level: EventLevel,
        unit_path: &[String],
        glyph_key: &str,
        message: &str,
    ) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ring) = inner.rings.get_mut(&reconcile_id) {
            let seq = ring.next_seq;
            ring.next_seq += 1;
            ring.events.push_back(ProgressEvent {
                seq,
                at: Utc::now(),
                level,
                unit_path: unit_path.to_vec(),
                glyph_key: glyph_key.to_string(),
                message: message.to_string(),
            });
            while ring.events.len() > EVENT_RING_CAP {
                ring.events.pop_front();
            }
        }
    }

    pub fn set_retry(&self, reconcile_id: u64, glyph_key: &str, ms: u64) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ring) = inner.rings.get_mut(&reconcile_id) {
            ring.retries.insert(glyph_key.to_string(), ms);
        }
    }

    pub fn clear_retry(&self, reconcile_id: u64, glyph_key: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ring) = inner.rings.get_mut(&reconcile_id) {
            ring.retries.remove(glyph_key);
        }
    }

    pub fn events_after(&self, reconcile_id: u64, after: u64) -> Vec<ProgressEvent> {
        let inner = self.inner.lock().unwrap();
        match inner.rings.get(&reconcile_id) {
            Some(ring) => ring
                .events
                .iter()
                .filter(|e| e.seq > after)
                .cloned()
                .collect(),
            None => Vec::new(),
        }
    }

    pub fn retries(&self, reconcile_id: u64) -> BTreeMap<String, u64> {
        let inner = self.inner.lock().unwrap();
        inner
            .rings
            .get(&reconcile_id)
            .map(|r| r.retries.clone())
            .unwrap_or_default()
    }
}

impl Default for ProgressRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_get_monotonic_seqs_and_after_returns_only_newer() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        reg.record(
            1,
            EventLevel::Info,
            &["scaly".into()],
            "apt:nginx",
            "install apt:nginx",
        );
        reg.record(
            1,
            EventLevel::Warn,
            &["scaly".into()],
            "apt:nginx",
            "enact failed (round 1)",
        );
        let all = reg.events_after(1, 0);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].seq, 1);
        assert_eq!(all[1].seq, 2);
        let tail = reg.events_after(1, 1);
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].seq, 2);
        assert!(matches!(tail[0].level, EventLevel::Warn));
    }

    #[test]
    fn the_ring_drops_oldest_past_the_cap_but_keeps_seq_monotone() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        for i in 0..(EVENT_RING_CAP as u64 + 5) {
            reg.record(
                1,
                EventLevel::Info,
                &["scaly".into()],
                "apt:x",
                &format!("line {i}"),
            );
        }
        let all = reg.events_after(1, 0);
        assert_eq!(all.len(), EVENT_RING_CAP);
        assert_eq!(all.last().unwrap().seq, EVENT_RING_CAP as u64 + 5);
        assert!(all.first().unwrap().seq > 1);
    }

    #[test]
    fn retry_countdown_is_set_and_cleared_per_glyph() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        reg.set_retry(1, "apt:x", 2000);
        assert_eq!(reg.retries(1).get("apt:x").copied(), Some(2000));
        reg.clear_retry(1, "apt:x");
        assert!(reg.retries(1).get("apt:x").is_none());
    }
}
