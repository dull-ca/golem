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
//! still holds. Eviction is **per-kind** (ADR 0033 §2 `kind` split): the
//! `lifecycle` stream and the high-volume `cmd` stream each carry their own
//! bound (`LIFECYCLE_RING_CAP`, `CMD_RING_CAP`) and evict only themselves, so a
//! command flood drops old `cmd` lines and never crowds out lifecycle events.
//! The `seq` cursor stays a single monotone stream across both kinds — one
//! ordered slice to the client — only the eviction bound is split. Only
//! `ATTEMPT_LRU` attempts are kept live at once — a poll targets the current or
//! just-finished attempt, so two is enough and a chatty rollback cannot bloat
//! daemon memory unbounded.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::Serialize;

/// The lifecycle-event bound: golemd's own decision lines (install/replace/
/// remove, enact-failed-round-N, giving-up, rollback, revision-recorded). A
/// handful per glyph, so a modest cap it never exhausts (ADR 0033 §2).
pub const LIFECYCLE_RING_CAP: usize = 1024;
/// The command-output bound: raw stdout/stderr lines of apt/systemd commands,
/// which a single `apt install` can push into the hundreds. Given a larger cap
/// so it evicts only itself under a flood (ADR 0033 §2).
pub const CMD_RING_CAP: usize = 4096;
const ATTEMPT_LRU: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventLevel {
    Info,
    Warn,
    Error,
}

/// Which stream an event belongs to (ADR 0033 §2). `Lifecycle` is golemd's
/// decision log; `Cmd` is the raw command output forwarded line by line. They
/// share one `seq` cursor but evict under separate bounds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Lifecycle,
    Cmd,
}

fn ring_cap(kind: EventKind) -> usize {
    match kind {
        EventKind::Lifecycle => LIFECYCLE_RING_CAP,
        EventKind::Cmd => CMD_RING_CAP,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ProgressEvent {
    pub seq: u64,
    pub at: DateTime<Utc>,
    pub level: EventLevel,
    pub kind: EventKind,
    pub unit_path: Vec<String>,
    pub glyph_key: String,
    pub message: String,
}

struct AttemptRing {
    next_seq: u64,
    events: VecDeque<ProgressEvent>,
    retries: BTreeMap<String, Instant>,
}

impl AttemptRing {
    fn new() -> Self {
        Self {
            next_seq: 1,
            events: VecDeque::new(),
            retries: BTreeMap::new(),
        }
    }

    fn evict(&mut self, kind: EventKind) {
        let cap = ring_cap(kind);
        while self.events.iter().filter(|e| e.kind == kind).count() > cap {
            if let Some(pos) = self.events.iter().position(|e| e.kind == kind) {
                self.events.remove(pos);
            } else {
                break;
            }
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
        if let std::collections::btree_map::Entry::Vacant(entry) = inner.rings.entry(reconcile_id) {
            entry.insert(AttemptRing::new());
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
        self.record_kind(
            reconcile_id,
            level,
            EventKind::Lifecycle,
            unit_path,
            glyph_key,
            message,
        );
    }

    pub fn record_kind(
        &self,
        reconcile_id: u64,
        level: EventLevel,
        kind: EventKind,
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
                kind,
                unit_path: unit_path.to_vec(),
                glyph_key: glyph_key.to_string(),
                message: message.to_string(),
            });
            ring.evict(kind);
        }
    }

    pub fn set_retry(&self, reconcile_id: u64, glyph_key: &str, delay: Duration) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(ring) = inner.rings.get_mut(&reconcile_id) {
            ring.retries
                .insert(glyph_key.to_string(), Instant::now() + delay);
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
        let now = Instant::now();
        let inner = self.inner.lock().unwrap();
        inner
            .rings
            .get(&reconcile_id)
            .map(|r| {
                r.retries
                    .iter()
                    .map(|(key, deadline)| {
                        (
                            key.clone(),
                            deadline.saturating_duration_since(now).as_millis() as u64,
                        )
                    })
                    .collect()
            })
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
        for i in 0..(LIFECYCLE_RING_CAP as u64 + 5) {
            reg.record(
                1,
                EventLevel::Info,
                &["scaly".into()],
                "apt:x",
                &format!("line {i}"),
            );
        }
        let all = reg.events_after(1, 0);
        assert_eq!(all.len(), LIFECYCLE_RING_CAP);
        assert_eq!(all.last().unwrap().seq, LIFECYCLE_RING_CAP as u64 + 5);
        assert!(all.first().unwrap().seq > 1);
    }

    #[test]
    fn a_cmd_flood_never_evicts_lifecycle_events() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        reg.record(
            1,
            EventLevel::Info,
            &["scaly".into()],
            "apt:podman",
            "install apt:podman",
        );
        for i in 0..(CMD_RING_CAP as u64 + 500) {
            reg.record_kind(
                1,
                EventLevel::Info,
                EventKind::Cmd,
                &["scaly".into()],
                "apt:podman",
                &format!("Unpacking chunk {i}"),
            );
        }
        let all = reg.events_after(1, 0);
        let lifecycle: Vec<_> = all
            .iter()
            .filter(|e| e.kind == EventKind::Lifecycle)
            .collect();
        let cmd: Vec<_> = all.iter().filter(|e| e.kind == EventKind::Cmd).collect();
        assert_eq!(
            lifecycle.len(),
            1,
            "the lone lifecycle event survives the cmd flood"
        );
        assert_eq!(lifecycle[0].message, "install apt:podman");
        assert_eq!(cmd.len(), CMD_RING_CAP, "cmd evicts only itself to its cap");
    }

    #[test]
    fn seq_stays_monotone_across_interleaved_kinds() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        reg.record(1, EventLevel::Info, &[], "apt:x", "install apt:x");
        reg.record_kind(
            1,
            EventLevel::Info,
            EventKind::Cmd,
            &[],
            "apt:x",
            "Unpacking x",
        );
        reg.record(1, EventLevel::Warn, &[], "apt:x", "enact failed");
        let all = reg.events_after(1, 0);
        let seqs: Vec<u64> = all.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2, 3]);
        let kinds: Vec<EventKind> = all.iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![EventKind::Lifecycle, EventKind::Cmd, EventKind::Lifecycle]
        );
    }

    #[test]
    fn retry_countdown_counts_down_from_the_deadline_and_clears() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        reg.set_retry(1, "apt:x", Duration::from_millis(2000));
        let remaining = reg.retries(1).get("apt:x").copied().unwrap();
        assert!(
            remaining > 1000 && remaining <= 2000,
            "a fresh 2s deadline reads as close to 2000ms, not more: {remaining}"
        );
        std::thread::sleep(Duration::from_millis(30));
        let later = reg.retries(1).get("apt:x").copied().unwrap();
        assert!(later < remaining, "the countdown decreases as time passes");
        reg.clear_retry(1, "apt:x");
        assert!(!reg.retries(1).contains_key("apt:x"));
    }

    #[test]
    fn an_elapsed_deadline_clamps_to_zero() {
        let reg = ProgressRegistry::new();
        reg.open(1);
        reg.set_retry(1, "apt:x", Duration::from_millis(0));
        std::thread::sleep(Duration::from_millis(5));
        assert_eq!(reg.retries(1).get("apt:x").copied(), Some(0));
    }
}
