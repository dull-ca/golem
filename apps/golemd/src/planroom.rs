//! golemd's local durable store, behind the [`PlanRoom`] port (ADR 0014 §4, ADR
//! 0020). Four tables:
//!
//! - `reconcile_attempt` and `wal_step` are the **write-ahead log** (ADR 0020).
//!   Both are append-only: one `reconcile_attempt` row per reconcile, opened
//!   before planning and phase-advanced as it runs; one `wal_step` row per state
//!   transition of a glyph op, never updated in place. This log is the
//!   authoritative record of what golem applied and how to reverse it.
//! - `applied_state` is now a rebuildable **cache**, not the source of truth.
//!   The applied set is derived by folding `wal_step` (`wal::applied_outcomes`);
//!   this single row only exists to hold the scroll and its content id, which the
//!   WAL does not carry, and is rewritten from the fold.
//! - `revisions` is the `GET /revisions` history. `Init` on first open, one
//!   `Reconcile` per committed attempt.
//!
//! The port's WAL surface — `open_attempt`/`set_attempt_phase`/`latest_attempt`/
//! `attempts` over attempts, `append_wal_step`/`wal_steps`/`wal_steps_for` over
//! steps — is what the foreman drives to bracket each side effect and what
//! recovery reads to fold and resume. `SqlitePlanRoom` is production;
//! `MemoryPlanRoom` gives tests the same semantics with no file (a shared
//! roundtrip test pins the two to identical behaviour). Both open with an `Init`
//! revision. Bodies are JSON for a legible log even though the wire format is
//! binary — the local store format is golemd's private choice (ADR 0014 §4).

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use scroll_format::ContentId;
use std::path::Path;
use std::sync::Mutex;

use crate::journal::{
    AppliedState, AttemptPhase, GlyphOp, Inverse, ReconcileAttempt, Revision, WalAction, WalStep,
    WalStepState,
};

/// Append to the WAL and revision log, read them back, and hold the applied-state
/// cache. Two adapters implement it identically (a shared roundtrip test pins
/// that); nothing above this port knows whether the store is sqlite or memory.
/// The methods split by table: `applied_state`/`put_applied_state` for the cache
/// row, `append_revision` and friends for history, `open_attempt`/
/// `set_attempt_phase`/`latest_attempt`/`attempts` for the attempt frames, and
/// `append_wal_step`/`wal_steps`/`wal_steps_for` for the per-op step log the
/// foreman brackets side effects with.
pub trait PlanRoom: Send + Sync {
    fn applied_state(&self) -> Result<Option<AppliedState>>;
    fn put_applied_state(&self, state: &AppliedState) -> Result<()>;
    fn revisions(&self) -> Result<Vec<Revision>>;
    fn revision(&self, id: u64) -> Result<Option<Revision>>;
    fn latest_revision_id(&self) -> Result<Option<u64>>;

    fn open_attempt(&self, scroll_content_id: Option<ContentId>) -> Result<ReconcileAttempt>;
    fn set_attempt_phase(&self, reconcile_id: u64, phase: AttemptPhase) -> Result<()>;
    fn latest_attempt(&self) -> Result<Option<ReconcileAttempt>>;
    fn attempts(&self) -> Result<Vec<ReconcileAttempt>>;

    #[allow(clippy::too_many_arguments)]
    fn append_wal_step(
        &self,
        reconcile_id: u64,
        step_ord: u64,
        glyph_key: &str,
        action: WalAction,
        state: WalStepState,
        op: &GlyphOp,
        inverse: Option<&Inverse>,
        changed: Option<bool>,
    ) -> Result<WalStep>;
    fn wal_steps(&self) -> Result<Vec<WalStep>>;
    fn wal_steps_for(&self, reconcile_id: u64) -> Result<Vec<WalStep>>;
}

impl<P: PlanRoom + ?Sized> PlanRoom for std::sync::Arc<P> {
    fn applied_state(&self) -> Result<Option<AppliedState>> {
        (**self).applied_state()
    }
    fn put_applied_state(&self, state: &AppliedState) -> Result<()> {
        (**self).put_applied_state(state)
    }
    fn revisions(&self) -> Result<Vec<Revision>> {
        (**self).revisions()
    }
    fn revision(&self, id: u64) -> Result<Option<Revision>> {
        (**self).revision(id)
    }
    fn latest_revision_id(&self) -> Result<Option<u64>> {
        (**self).latest_revision_id()
    }
    fn open_attempt(&self, scroll_content_id: Option<ContentId>) -> Result<ReconcileAttempt> {
        (**self).open_attempt(scroll_content_id)
    }
    fn set_attempt_phase(&self, reconcile_id: u64, phase: AttemptPhase) -> Result<()> {
        (**self).set_attempt_phase(reconcile_id, phase)
    }
    fn latest_attempt(&self) -> Result<Option<ReconcileAttempt>> {
        (**self).latest_attempt()
    }
    fn attempts(&self) -> Result<Vec<ReconcileAttempt>> {
        (**self).attempts()
    }
    fn append_wal_step(
        &self,
        reconcile_id: u64,
        step_ord: u64,
        glyph_key: &str,
        action: WalAction,
        state: WalStepState,
        op: &GlyphOp,
        inverse: Option<&Inverse>,
        changed: Option<bool>,
    ) -> Result<WalStep> {
        (**self).append_wal_step(reconcile_id, step_ord, glyph_key, action, state, op, inverse, changed)
    }
    fn wal_steps(&self) -> Result<Vec<WalStep>> {
        (**self).wal_steps()
    }
    fn wal_steps_for(&self, reconcile_id: u64) -> Result<Vec<WalStep>> {
        (**self).wal_steps_for(reconcile_id)
    }
}

/// The on-disk store: a WAL-mode sqlite file. `synchronous = NORMAL` on WAL mode
/// makes each `INSERT` durable at commit, which is what the intent-before/
/// outcome-after bracketing relies on — a step's `Intended` row is on disk before
/// the reconciler runs (each `append_wal_step` is its own committed transaction,
/// never one transaction spanning the attempt). Adds the two ADR 0020 tables to
/// the existing `applied_state` cache row and `revisions` log; `CREATE TABLE IF
/// NOT EXISTS` makes reopening an older database a no-op.
pub struct SqlitePlanRoom {
    conn: Mutex<Connection>,
}

impl SqlitePlanRoom {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path).context("open sqlite")?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous  = FULL;
            CREATE TABLE IF NOT EXISTS applied_state (
                id   INTEGER PRIMARY KEY CHECK (id = 0),
                body TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reconcile_attempt (
                reconcile_id      INTEGER PRIMARY KEY AUTOINCREMENT,
                started_at        TEXT NOT NULL,
                scroll_content_id TEXT,
                phase             TEXT NOT NULL,
                settled_at        TEXT
            );
            CREATE TABLE IF NOT EXISTS wal_step (
                seq          INTEGER PRIMARY KEY AUTOINCREMENT,
                reconcile_id INTEGER NOT NULL REFERENCES reconcile_attempt(reconcile_id),
                step_ord     INTEGER NOT NULL,
                glyph_key    TEXT NOT NULL,
                action       TEXT NOT NULL,
                state        TEXT NOT NULL,
                op           TEXT NOT NULL,
                inverse      TEXT,
                changed      INTEGER,
                at           TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS wal_step_by_attempt
                ON wal_step(reconcile_id, step_ord, seq);
            "#,
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }
}

impl PlanRoom for SqlitePlanRoom {
    fn applied_state(&self) -> Result<Option<AppliedState>> {
        let conn = self.conn.lock().unwrap();
        let body: Option<String> = conn
            .query_row("SELECT body FROM applied_state WHERE id = 0", [], |r| r.get(0))
            .optional()?;
        match body {
            Some(body) => Ok(Some(serde_json::from_str(&body).context("decode applied state")?)),
            None => Ok(None),
        }
    }

    fn put_applied_state(&self, state: &AppliedState) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO applied_state(id, body) VALUES(0, ?1)
             ON CONFLICT(id) DO UPDATE SET body = excluded.body",
            params![serde_json::to_string(state)?],
        )?;
        Ok(())
    }

    fn revisions(&self) -> Result<Vec<Revision>> {
        Ok(crate::wal::projected_revisions(&self.attempts()?, &self.wal_steps()?))
    }

    fn revision(&self, id: u64) -> Result<Option<Revision>> {
        Ok(crate::wal::projected_revision(&self.attempts()?, &self.wal_steps()?, id))
    }

    fn latest_revision_id(&self) -> Result<Option<u64>> {
        Ok(crate::wal::latest_revision_id(&self.attempts()?))
    }

    fn open_attempt(&self, scroll_content_id: Option<ContentId>) -> Result<ReconcileAttempt> {
        let now = Utc::now();
        let cid_token = scroll_content_id.map(|c| c.to_string());
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO reconcile_attempt(started_at, scroll_content_id, phase, settled_at)
             VALUES(?1, ?2, ?3, NULL)",
            params![now.to_rfc3339(), cid_token, phase_token(AttemptPhase::Planning)],
        )?;
        Ok(ReconcileAttempt {
            reconcile_id: conn.last_insert_rowid() as u64,
            started_at: now,
            scroll_content_id,
            phase: AttemptPhase::Planning,
            settled_at: None,
        })
    }

    fn set_attempt_phase(&self, reconcile_id: u64, phase: AttemptPhase) -> Result<()> {
        let settled_at = phase.is_settled().then(|| Utc::now().to_rfc3339());
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE reconcile_attempt
             SET phase = ?1,
                 settled_at = COALESCE(?2, settled_at)
             WHERE reconcile_id = ?3",
            params![phase_token(phase), settled_at, reconcile_id as i64],
        )?;
        Ok(())
    }

    fn latest_attempt(&self) -> Result<Option<ReconcileAttempt>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT reconcile_id, started_at, scroll_content_id, phase, settled_at
             FROM reconcile_attempt ORDER BY reconcile_id DESC LIMIT 1",
            [],
            row_to_attempt,
        )
        .optional()
        .map_err(Into::into)
    }

    fn attempts(&self) -> Result<Vec<ReconcileAttempt>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT reconcile_id, started_at, scroll_content_id, phase, settled_at
             FROM reconcile_attempt ORDER BY reconcile_id ASC",
        )?;
        let rows = stmt.query_map([], row_to_attempt)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn append_wal_step(
        &self,
        reconcile_id: u64,
        step_ord: u64,
        glyph_key: &str,
        action: WalAction,
        state: WalStepState,
        op: &GlyphOp,
        inverse: Option<&Inverse>,
        changed: Option<bool>,
    ) -> Result<WalStep> {
        let now = Utc::now();
        let inverse_token = match inverse {
            Some(i) => Some(serde_json::to_string(i)?),
            None => None,
        };
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO wal_step(reconcile_id, step_ord, glyph_key, action, state, op, inverse, changed, at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                reconcile_id as i64,
                step_ord as i64,
                glyph_key,
                action_token(action),
                step_state_token(state),
                serde_json::to_string(op)?,
                inverse_token,
                changed.map(|c| c as i64),
                now.to_rfc3339(),
            ],
        )?;
        Ok(WalStep {
            seq: conn.last_insert_rowid() as u64,
            reconcile_id,
            step_ord,
            glyph_key: glyph_key.to_string(),
            action,
            state,
            op: op.clone(),
            inverse: inverse.cloned(),
            changed,
            at: now,
        })
    }

    fn wal_steps(&self) -> Result<Vec<WalStep>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, reconcile_id, step_ord, glyph_key, action, state, op, inverse, changed, at
             FROM wal_step ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map([], row_to_wal_step)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn wal_steps_for(&self, reconcile_id: u64) -> Result<Vec<WalStep>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT seq, reconcile_id, step_ord, glyph_key, action, state, op, inverse, changed, at
             FROM wal_step WHERE reconcile_id = ?1 ORDER BY seq ASC",
        )?;
        let rows = stmt.query_map(params![reconcile_id as i64], row_to_wal_step)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }
}

fn phase_token(phase: AttemptPhase) -> String {
    serde_json::to_value(phase)
        .expect("AttemptPhase serializes")
        .as_str()
        .expect("AttemptPhase serializes as a string")
        .to_string()
}

fn action_token(action: WalAction) -> String {
    serde_json::to_value(action)
        .expect("WalAction serializes")
        .as_str()
        .expect("WalAction serializes as a string")
        .to_string()
}

fn step_state_token(state: WalStepState) -> String {
    serde_json::to_value(state)
        .expect("WalStepState serializes")
        .as_str()
        .expect("WalStepState serializes as a string")
        .to_string()
}

fn row_to_attempt(r: &rusqlite::Row) -> rusqlite::Result<ReconcileAttempt> {
    let conv = |col, e: Box<dyn std::error::Error + Send + Sync>| {
        rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, e)
    };
    let started_at: String = r.get(1)?;
    let started_at = chrono::DateTime::parse_from_rfc3339(&started_at)
        .map_err(|e| conv(1, Box::new(e)))?
        .with_timezone(&Utc);
    let cid: Option<String> = r.get(2)?;
    let scroll_content_id = match cid {
        Some(s) => Some(s.parse::<ContentId>().map_err(|e| conv(2, Box::new(e)))?),
        None => None,
    };
    let phase: AttemptPhase = serde_json::from_value(serde_json::Value::String(r.get(3)?))
        .map_err(|e| conv(3, Box::new(e)))?;
    let settled: Option<String> = r.get(4)?;
    let settled_at = match settled {
        Some(s) => Some(
            chrono::DateTime::parse_from_rfc3339(&s)
                .map_err(|e| conv(4, Box::new(e)))?
                .with_timezone(&Utc),
        ),
        None => None,
    };
    Ok(ReconcileAttempt {
        reconcile_id: r.get::<_, i64>(0)? as u64,
        started_at,
        scroll_content_id,
        phase,
        settled_at,
    })
}

fn row_to_wal_step(r: &rusqlite::Row) -> rusqlite::Result<WalStep> {
    let conv = |col, e: Box<dyn std::error::Error + Send + Sync>| {
        rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, e)
    };
    let action: WalAction = serde_json::from_value(serde_json::Value::String(r.get(4)?))
        .map_err(|e| conv(4, Box::new(e)))?;
    let state: WalStepState = serde_json::from_value(serde_json::Value::String(r.get(5)?))
        .map_err(|e| conv(5, Box::new(e)))?;
    let op: GlyphOp =
        serde_json::from_str(&r.get::<_, String>(6)?).map_err(|e| conv(6, Box::new(e)))?;
    let inverse: Option<String> = r.get(7)?;
    let inverse = match inverse {
        Some(s) => Some(serde_json::from_str(&s).map_err(|e| conv(7, Box::new(e)))?),
        None => None,
    };
    let changed: Option<i64> = r.get(8)?;
    let at: String = r.get(9)?;
    let at = chrono::DateTime::parse_from_rfc3339(&at)
        .map_err(|e| conv(9, Box::new(e)))?
        .with_timezone(&Utc);
    Ok(WalStep {
        seq: r.get::<_, i64>(0)? as u64,
        reconcile_id: r.get::<_, i64>(1)? as u64,
        step_ord: r.get::<_, i64>(2)? as u64,
        glyph_key: r.get(3)?,
        action,
        state,
        op,
        inverse,
        changed: changed.map(|c| c != 0),
        at,
    })
}

#[derive(Default)]
struct Inner {
    applied: Option<AppliedState>,
    attempts: Vec<ReconcileAttempt>,
    wal: Vec<WalStep>,
}

/// The in-memory plan room used by tests: the same behaviour as
/// [`SqlitePlanRoom`] with no file.
pub struct MemoryPlanRoom {
    inner: Mutex<Inner>,
}

impl MemoryPlanRoom {
    pub fn new() -> Self {
        Self { inner: Mutex::new(Inner::default()) }
    }
}

impl Default for MemoryPlanRoom {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanRoom for MemoryPlanRoom {
    fn applied_state(&self) -> Result<Option<AppliedState>> {
        Ok(self.inner.lock().unwrap().applied.clone())
    }

    fn put_applied_state(&self, state: &AppliedState) -> Result<()> {
        self.inner.lock().unwrap().applied = Some(state.clone());
        Ok(())
    }

    fn revisions(&self) -> Result<Vec<Revision>> {
        let inner = self.inner.lock().unwrap();
        Ok(crate::wal::projected_revisions(&inner.attempts, &inner.wal))
    }

    fn revision(&self, id: u64) -> Result<Option<Revision>> {
        let inner = self.inner.lock().unwrap();
        Ok(crate::wal::projected_revision(&inner.attempts, &inner.wal, id))
    }

    fn latest_revision_id(&self) -> Result<Option<u64>> {
        let inner = self.inner.lock().unwrap();
        Ok(crate::wal::latest_revision_id(&inner.attempts))
    }

    fn open_attempt(&self, scroll_content_id: Option<ContentId>) -> Result<ReconcileAttempt> {
        let mut inner = self.inner.lock().unwrap();
        let attempt = ReconcileAttempt {
            reconcile_id: inner.attempts.len() as u64 + 1,
            started_at: Utc::now(),
            scroll_content_id,
            phase: AttemptPhase::Planning,
            settled_at: None,
        };
        inner.attempts.push(attempt.clone());
        Ok(attempt)
    }

    fn set_attempt_phase(&self, reconcile_id: u64, phase: AttemptPhase) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        if let Some(a) = inner.attempts.iter_mut().find(|a| a.reconcile_id == reconcile_id) {
            a.phase = phase;
            if phase.is_settled() && a.settled_at.is_none() {
                a.settled_at = Some(Utc::now());
            }
        }
        Ok(())
    }

    fn latest_attempt(&self) -> Result<Option<ReconcileAttempt>> {
        Ok(self.inner.lock().unwrap().attempts.last().cloned())
    }

    fn attempts(&self) -> Result<Vec<ReconcileAttempt>> {
        Ok(self.inner.lock().unwrap().attempts.clone())
    }

    fn append_wal_step(
        &self,
        reconcile_id: u64,
        step_ord: u64,
        glyph_key: &str,
        action: WalAction,
        state: WalStepState,
        op: &GlyphOp,
        inverse: Option<&Inverse>,
        changed: Option<bool>,
    ) -> Result<WalStep> {
        let mut inner = self.inner.lock().unwrap();
        let step = WalStep {
            seq: inner.wal.len() as u64 + 1,
            reconcile_id,
            step_ord,
            glyph_key: glyph_key.to_string(),
            action,
            state,
            op: op.clone(),
            inverse: inverse.cloned(),
            changed,
            at: Utc::now(),
        };
        inner.wal.push(step.clone());
        Ok(step)
    }

    fn wal_steps(&self) -> Result<Vec<WalStep>> {
        Ok(self.inner.lock().unwrap().wal.clone())
    }

    fn wal_steps_for(&self, reconcile_id: u64) -> Result<Vec<WalStep>> {
        Ok(self
            .inner
            .lock()
            .unwrap()
            .wal
            .iter()
            .filter(|s| s.reconcile_id == reconcile_id)
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scroll_format::{Glyph, Scroll};

    fn sample() -> AppliedState {
        let scroll = Scroll { name: "h1".into(), glyphs: vec![Glyph::AptPackage { name: "nginx".into() }] };
        AppliedState {
            scroll_content_id: scroll_format::content_id(&scroll),
            scroll,
            outcomes: vec![],
        }
    }

    fn roundtrip(room: &dyn PlanRoom) {
        use crate::journal::RevisionKind;
        assert_eq!(room.latest_revision_id().unwrap(), Some(1), "starts with Init");
        assert!(room.applied_state().unwrap().is_none());

        room.put_applied_state(&sample()).unwrap();
        assert_eq!(room.applied_state().unwrap().unwrap(), sample());

        room.open_attempt(Some(sample().scroll_content_id)).unwrap();
        room.set_attempt_phase(1, AttemptPhase::Committed).unwrap();

        assert_eq!(room.latest_revision_id().unwrap(), Some(2));
        let rev = room.revision(2).unwrap().unwrap();
        assert_eq!(rev.kind, RevisionKind::Reconcile);
        assert_eq!(rev.scroll_content_id, Some(sample().scroll_content_id));
        assert_eq!(room.revisions().unwrap().len(), 2);
        assert!(room.revision(9_999).unwrap().is_none());
    }

    #[test]
    fn sqlite_and_memory_behave_the_same() {
        roundtrip(&MemoryPlanRoom::new());
        roundtrip(&SqlitePlanRoom::open(Path::new(":memory:")).unwrap());
    }

    fn apt_op(name: &str) -> GlyphOp {
        let glyph = Glyph::AptPackage { name: name.into() };
        GlyphOp::Install { cid: scroll_format::content_id_of_glyph(&glyph), glyph }
    }

    fn wal_roundtrip(room: &dyn PlanRoom) {
        assert!(room.latest_attempt().unwrap().is_none());

        let cid = sample().scroll_content_id;
        let attempt = room.open_attempt(Some(cid)).unwrap();
        assert_eq!(attempt.reconcile_id, 1);
        assert_eq!(attempt.phase, AttemptPhase::Planning);
        assert!(attempt.settled_at.is_none());
        assert_eq!(room.latest_attempt().unwrap().unwrap().reconcile_id, 1);

        room.set_attempt_phase(1, AttemptPhase::Enacting).unwrap();
        assert_eq!(room.latest_attempt().unwrap().unwrap().phase, AttemptPhase::Enacting);

        let op = apt_op("nginx");
        let intended = room
            .append_wal_step(1, 0, "apt:nginx", WalAction::Apply, WalStepState::Intended, &op, None, None)
            .unwrap();
        assert_eq!(intended.seq, 1);
        assert_eq!(intended.state, WalStepState::Intended);

        let done = room
            .append_wal_step(
                1,
                0,
                "apt:nginx",
                WalAction::Apply,
                WalStepState::Done,
                &op,
                Some(&Inverse::RemoveAptPackage { name: "nginx".into() }),
                Some(true),
            )
            .unwrap();
        assert_eq!(done.seq, 2);

        let steps = room.wal_steps_for(1).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].state, WalStepState::Intended);
        assert_eq!(steps[1].state, WalStepState::Done);
        assert_eq!(steps[1].inverse, Some(Inverse::RemoveAptPackage { name: "nginx".into() }));
        assert_eq!(steps[1].changed, Some(true));

        assert_eq!(room.wal_steps().unwrap().len(), 2);

        room.set_attempt_phase(1, AttemptPhase::Committed).unwrap();
        let settled = room.latest_attempt().unwrap().unwrap();
        assert_eq!(settled.phase, AttemptPhase::Committed);
        assert!(settled.settled_at.is_some());
        assert_eq!(room.attempts().unwrap().len(), 1);
    }

    #[test]
    fn sqlite_and_memory_wal_behave_the_same() {
        wal_roundtrip(&MemoryPlanRoom::new());
        wal_roundtrip(&SqlitePlanRoom::open(Path::new(":memory:")).unwrap());
    }

    #[test]
    fn sqlite_commits_are_fsynced_for_power_loss_durability() {
        let room = SqlitePlanRoom::open(Path::new(":memory:")).unwrap();
        let synchronous: i64 = room
            .conn
            .lock()
            .unwrap()
            .query_row("PRAGMA synchronous", [], |r| r.get(0))
            .unwrap();
        assert_eq!(synchronous, 2, "synchronous = FULL fsyncs each WAL commit");
    }
}
