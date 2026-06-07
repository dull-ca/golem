//! The plan room: a golem's local store of Blueprints plus the Revision
//! journal. [`PlanRoom`] is the trait; [`SqlitePlanRoom`] is the durable impl,
//! [`MemoryPlanRoom`] the in-memory one.

use anyhow::{Context, Result};
use chrono::Utc;
use golem_types::{Action, Blueprint, Revision, RevisionKind, State};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

pub trait PlanRoom: Send + Sync {
    fn blueprints(&self) -> Result<BTreeMap<String, Blueprint>>;
    fn put_blueprint(&self, bp: &Blueprint) -> Result<()>;
    fn delete_blueprint(&self, name: &str) -> Result<()>;
    fn append_revision(
        &self,
        kind: RevisionKind,
        blueprint: Option<String>,
        actions: &[Action],
        state: &State,
    ) -> Result<Revision>;
    fn revisions(&self) -> Result<Vec<Revision>>;
    fn revision(&self, id: u64) -> Result<Option<Revision>>;
    fn latest_revision_id(&self) -> Result<Option<u64>>;
}

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
            CREATE TABLE IF NOT EXISTS blueprints (
                name TEXT PRIMARY KEY,
                body TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS revisions (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                kind      TEXT NOT NULL,
                blueprint TEXT,
                actions   TEXT NOT NULL,
                state     TEXT NOT NULL
            );
            "#,
        )?;
        let room = Self { conn: Mutex::new(conn) };
        if room.latest_revision_id()?.is_none() {
            room.append_revision(RevisionKind::Init, None, &[], &State::default())?;
        }
        Ok(room)
    }
}

impl PlanRoom for SqlitePlanRoom {
    fn blueprints(&self) -> Result<BTreeMap<String, Blueprint>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name, body FROM blueprints")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = BTreeMap::new();
        for row in rows {
            let (name, body) = row?;
            let bp = serde_json::from_str(&body).with_context(|| format!("decode blueprint {name}"))?;
            out.insert(name, bp);
        }
        Ok(out)
    }

    fn put_blueprint(&self, bp: &Blueprint) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO blueprints(name, body) VALUES(?1, ?2)
             ON CONFLICT(name) DO UPDATE SET body = excluded.body",
            params![bp.name, serde_json::to_string(bp)?],
        )?;
        Ok(())
    }

    fn delete_blueprint(&self, name: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM blueprints WHERE name = ?1", params![name])?;
        Ok(())
    }

    fn append_revision(
        &self,
        kind: RevisionKind,
        blueprint: Option<String>,
        actions: &[Action],
        state: &State,
    ) -> Result<Revision> {
        let now = Utc::now();
        let kind_token = serde_json::to_value(kind)?;
        let kind_token = kind_token.as_str().expect("RevisionKind serializes as a string");
        let id = {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO revisions(created_at, kind, blueprint, actions, state) VALUES(?1,?2,?3,?4,?5)",
                params![
                    now.to_rfc3339(),
                    kind_token,
                    blueprint,
                    serde_json::to_string(actions)?,
                    serde_json::to_string(state)?,
                ],
            )?;
            conn.last_insert_rowid() as u64
        };
        Ok(Revision { id, created_at: now, kind, blueprint, actions: actions.to_vec(), state: state.clone() })
    }

    fn revisions(&self) -> Result<Vec<Revision>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, created_at, kind, blueprint, actions, state FROM revisions ORDER BY id ASC")?;
        let rows = stmt.query_map([], row_to_revision)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    fn revision(&self, id: u64) -> Result<Option<Revision>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, created_at, kind, blueprint, actions, state FROM revisions WHERE id = ?1",
            params![id as i64],
            row_to_revision,
        )
        .optional()
        .map_err(Into::into)
    }

    fn latest_revision_id(&self) -> Result<Option<u64>> {
        let conn = self.conn.lock().unwrap();
        let id: Option<i64> =
            conn.query_row("SELECT MAX(id) FROM revisions", [], |r| r.get(0)).optional()?.flatten();
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
    Ok(Revision {
        id: r.get::<_, i64>(0)? as u64,
        created_at,
        kind,
        blueprint: r.get(3)?,
        actions: serde_json::from_str(&r.get::<_, String>(4)?).map_err(|e| conv(4, Box::new(e)))?,
        state: serde_json::from_str(&r.get::<_, String>(5)?).map_err(|e| conv(5, Box::new(e)))?,
    })
}

#[derive(Default)]
struct Inner {
    blueprints: BTreeMap<String, Blueprint>,
    revisions: Vec<Revision>,
}

pub struct MemoryPlanRoom {
    inner: Mutex<Inner>,
}

impl MemoryPlanRoom {
    pub fn new() -> Self {
        let room = Self { inner: Mutex::new(Inner::default()) };
        room.append_revision(RevisionKind::Init, None, &[], &State::default()).expect("init");
        room
    }
}

impl Default for MemoryPlanRoom {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanRoom for MemoryPlanRoom {
    fn blueprints(&self) -> Result<BTreeMap<String, Blueprint>> {
        Ok(self.inner.lock().unwrap().blueprints.clone())
    }

    fn put_blueprint(&self, bp: &Blueprint) -> Result<()> {
        self.inner.lock().unwrap().blueprints.insert(bp.name.clone(), bp.clone());
        Ok(())
    }

    fn delete_blueprint(&self, name: &str) -> Result<()> {
        self.inner.lock().unwrap().blueprints.remove(name);
        Ok(())
    }

    fn append_revision(
        &self,
        kind: RevisionKind,
        blueprint: Option<String>,
        actions: &[Action],
        state: &State,
    ) -> Result<Revision> {
        let mut inner = self.inner.lock().unwrap();
        let rev = Revision {
            id: inner.revisions.len() as u64 + 1,
            created_at: Utc::now(),
            kind,
            blueprint,
            actions: actions.to_vec(),
            state: state.clone(),
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

    fn sample() -> Blueprint {
        Blueprint { name: "web".into(), hosts: vec![] }
    }

    fn roundtrip(room: &dyn PlanRoom) {
        assert_eq!(room.latest_revision_id().unwrap(), Some(1), "starts with Init");

        room.put_blueprint(&sample()).unwrap();
        room.put_blueprint(&sample()).unwrap(); // upsert: same name, still one
        assert_eq!(room.blueprints().unwrap().len(), 1);

        let rev = room
            .append_revision(RevisionKind::Commission, Some("web".into()), &[], &State::default())
            .unwrap();
        assert_eq!(room.revision(rev.id).unwrap().unwrap(), rev);
        assert_eq!(room.latest_revision_id().unwrap(), Some(rev.id));
        assert!(room.revision(9_999).unwrap().is_none());

        room.delete_blueprint("web").unwrap();
        assert!(room.blueprints().unwrap().is_empty());
    }

    #[test]
    fn sqlite_and_memory_behave_the_same() {
        roundtrip(&MemoryPlanRoom::new());
        roundtrip(&SqlitePlanRoom::open(Path::new(":memory:")).unwrap());
    }
}
