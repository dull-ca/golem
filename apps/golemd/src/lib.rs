//! golemd: the per-host agent that enacts a content-addressed scroll.
//!
//! It ingests a binary manifest (the `scroll-format` wire contract), selects
//! this host's scroll, diffs it against the last applied state by content id
//! (`reconcile`), and enacts the resulting glyph ops through the `Reconciler`
//! port with reversible outcomes journalled for exact undo (ADR 0014/0015/0020).
//! The ports are `Reconciler` (enact a glyph — real host adapters or the fake)
//! and `PlanRoom` (the write-ahead log plus a rebuildable applied-state cache);
//! `foreman` is the reconcile loop wiring them, and `http` drives it. The
//! revision journal is not stored but projected from the settled WAL at read time
//! (`wal::projected_revisions`, ADR 0020 §6).

pub mod config;
pub mod fake_reconciler;
pub mod foreman;
pub mod host;
pub mod http;
pub mod journal;
pub mod plan_report;
pub mod planroom;
pub mod progress;
pub mod projection;
pub mod reconcile;
pub mod reconciler;
pub mod reconcilers;
pub mod report;
pub mod secrets;
pub mod wal;
