//! The plan room: golemd's local store of the current applied state plus the
//! append-only revision journal (ADR 0014 §4). One record of applied state
//! (the last scroll this node accepted, overwritten each reconcile) and an
//! ever-growing revision log. The [`PlanRoom`] port has a `SqlitePlanRoom` for
//! production and a `MemoryPlanRoom` for tests; both open with an `Init`
//! revision. Bodies are stored as JSON for a legible journal even though the
//! wire format is binary (ADR 0014 §4 — the local journal format is golemd's
//! private choice).

use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use scroll_format::ContentId;
use std::path::Path;
use std::sync::Mutex;

use crate::journal::{AppliedState, Outcome, Revision, RevisionKind};

/// Read/write the current applied state and append to the revision journal. Two
/// adapters implement it identically (a shared roundtrip test pins that);
/// nothing above this port knows whether state lives in sqlite or memory.
pub trait PlanRoom: Send + Sync {
    fn applied_state(&self) -> Result<Option<AppliedState>>;
    fn put_applied_state(&self, state: &AppliedState) -> Result<()>;
    fn append_revision(
        &self,
        kind: RevisionKind,
        scroll_content_id: Option<ContentId>,
        outcomes: &[Outcome],
    ) -> Result<Revision>;
    fn revisions(&self) -> Result<Vec<Revision>>;
    fn revision(&self, id: u64) -> Result<Option<Revision>>;
    fn latest_revision_id(&self) -> Result<Option<u64>>;
}

/// The on-disk plan room: a WAL-mode sqlite file with a single-row
/// `applied_state` table and an autoincrement `revisions` log.
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
            PRAGMA synchronous  = NORMAL;
            CREATE TABLE IF NOT EXISTS applied_state (
                id   INTEGER PRIMARY KEY CHECK (id = 0),
                body TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS revisions (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at        TEXT NOT NULL,
                kind              TEXT NOT NULL,
                scroll_content_id TEXT,
                outcomes          TEXT NOT NULL
            );
            "#,
        )?;
        let room = Self { conn: Mutex::new(conn) };
        if room.latest_revision_id()?.is_none() {
            room.append_revision(RevisionKind::Init, None, &[])?;
        }
        Ok(room)
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

    fn append_revision(
        &self,
        kind: RevisionKind,
        scroll_content_id: Option<ContentId>,
        outcomes: &[Outcome],
    ) -> Result<Revision> {
        let now = Utc::now();
        let kind_token = serde_json::to_value(kind)?;
        let kind_token = kind_token.as_str().expect("RevisionKind serializes as a string");
        let cid_token = scroll_content_id.map(|c| c.to_string());
        let id = {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO revisions(created_at, kind, scroll_content_id, outcomes) VALUES(?1,?2,?3,?4)",
                params![
                    now.to_rfc3339(),
                    kind_token,
                    cid_token,
                    serde_json::to_string(outcomes)?,
                ],
            )?;
            conn.last_insert_rowid() as u64
        };
        Ok(Revision {
            id,
            created_at: now,
            kind,
            scroll_content_id,
            outcomes: outcomes.to_vec(),
        })
    }

    fn revisions(&self) -> Result<Vec<Revision>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, kind, scroll_content_id, outcomes FROM revisions ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], row_to_revision)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn revision(&self, id: u64) -> Result<Option<Revision>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, created_at, kind, scroll_content_id, outcomes FROM revisions WHERE id = ?1",
            params![id as i64],
            row_to_revision,
        )
        .optional()
        .map_err(Into::into)
    }

    fn latest_revision_id(&self) -> Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let id: Option<i64> = conn
            .query_row("SELECT MAX(id) FROM revisions", [], |r| r.get(0))
            .optional()?
            .flatten();
        Ok(id.map(|v| v as u64))
    }
}

fn row_to_revision(r: &rusqlite::Row) -> rusqlite::Result<Revision> {
    let conv = |col, e: Box<dyn std::error::Error + Send + Sync>| {
        rusqlite::Error::FromSqlConversionFailure(col, rusqlite::types::Type::Text, e)
    };
    let created_at: String = r.get(1)?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&created_at)
        .map_err(|e| conv(1, Box::new(e)))?
        .with_timezone(&Utc);
    let kind: RevisionKind = serde_json::from_value(serde_json::Value::String(r.get(2)?))
        .map_err(|e| conv(2, Box::new(e)))?;
    let cid: Option<String> = r.get(3)?;
    let scroll_content_id = match cid {
        Some(s) => Some(s.parse::<ContentId>().map_err(|e| conv(3, Box::new(e)))?),
        None => None,
    };
    Ok(Revision {
        id: r.get::<_, i64>(0)? as u64,
        created_at,
        kind,
        scroll_content_id,
        outcomes: serde_json::from_str(&r.get::<_, String>(4)?).map_err(|e| conv(4, Box::new(e)))?,
    })
}

#[derive(Default)]
struct Inner {
    applied: Option<AppliedState>,
    revisions: Vec<Revision>,
}

/// The in-memory plan room used by tests: the same behaviour as
/// [`SqlitePlanRoom`] with no file.
pub struct MemoryPlanRoom {
    inner: Mutex<Inner>,
}

impl MemoryPlanRoom {
    pub fn new() -> Self {
        let room = Self { inner: Mutex::new(Inner::default()) };
        room.append_revision(RevisionKind::Init, None, &[]).expect("init");
        room
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

    fn append_revision(
        &self,
        kind: RevisionKind,
        scroll_content_id: Option<ContentId>,
        outcomes: &[Outcome],
    ) -> Result<Revision> {
        let mut inner = self.inner.lock().unwrap();
        let rev = Revision {
            id: inner.revisions.len() as u64 + 1,
            created_at: Utc::now(),
            kind,
            scroll_content_id,
            outcomes: outcomes.to_vec(),
        };
        inner.revisions.push(rev.clone());
        Ok(rev)
    }

    fn revisions(&self) -> Result<Vec<Revision>> {
        Ok(self.inner.lock().unwrap().revisions.clone())
    }

    fn revision(&self, id: u64) -> Result<Option<Revision>> {
        Ok(self.inner.lock().unwrap().revisions.iter().find(|r| r.id == id).cloned())
    }

    fn latest_revision_id(&self) -> Result<Option<u64>> {
        Ok(self.inner.lock().unwrap().revisions.last().map(|r| r.id))
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
        assert_eq!(room.latest_revision_id().unwrap(), Some(1), "starts with Init");
        assert!(room.applied_state().unwrap().is_none());

        room.put_applied_state(&sample()).unwrap();
        assert_eq!(room.applied_state().unwrap().unwrap(), sample());

        let rev = room
            .append_revision(RevisionKind::Reconcile, Some(sample().scroll_content_id), &[])
            .unwrap();
        assert_eq!(room.revision(rev.id).unwrap().unwrap(), rev);
        assert_eq!(room.latest_revision_id().unwrap(), Some(rev.id));
        assert!(room.revision(9_999).unwrap().is_none());
    }

    #[test]
    fn sqlite_and_memory_behave_the_same() {
        roundtrip(&MemoryPlanRoom::new());
        roundtrip(&SqlitePlanRoom::open(Path::new(":memory:")).unwrap());
    }
}
