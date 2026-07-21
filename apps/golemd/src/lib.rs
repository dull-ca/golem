//! golemd: the per-host agent that enacts a content-addressed scroll.
//!
//! It ingests a binary manifest (the `scroll-format` wire contract), selects
//! this host's scroll, diffs it against the last applied state by content id
//! (`reconcile`), and enacts the resulting glyph ops through the `Reconciler`
//! port with reversible outcomes journalled for exact undo (ADR 0014/0015). The
//! ports are `Reconciler` (enact a glyph — real host adapters or the fake) and
//! `PlanRoom` (store applied state + the revision journal); `foreman` is the
//! reconcile loop wiring them, and `http` drives it.

pub mod fake_reconciler;
pub mod foreman;
pub mod host;
pub mod http;
pub mod journal;
pub mod planroom;
pub mod reconcile;
pub mod reconciler;
pub mod reconcilers;
pub mod wal;
