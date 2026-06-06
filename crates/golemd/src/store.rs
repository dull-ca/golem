//! Persistence: blueprints + revisions in SQLite.
//!
//! Two tables. `blueprints` is current — keyed by name, replaced on
//! re-commission, deleted on decommission. `revisions` is append-only;
//! every mutation appends one row carrying the resolved state and the
//! actions that would transition into it.

use anyhow::{Context, Result};
use chrono::Utc;
use golem_types::{Action, Blueprint, Revision, RevisionKind, State};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path).context("open sqlite")?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous  = NORMAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS blueprints (
                name TEXT PRIMARY KEY,
                body TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS revisions (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                at           TEXT    NOT NULL,
                kind         TEXT    NOT NULL,
                blueprint    TEXT,
                actions      TEXT    NOT NULL,
                state        TEXT    NOT NULL
            );
            "#,
        )?;

        let store = Self {
            conn: Mutex::new(conn),
        };

        // Write the init revision once, on first boot.
        if store.latest_revision_id()? == 0 {
            store.append_revision(RevisionKind::Init, None, &[], &State::default())?;
        }
        Ok(store)
    }

    fn latest_revision_id(&self) -> Result<u64> {
        let conn = self.conn.lock().unwrap();
        let id: Option<i64> = conn
            .query_row("SELECT MAX(id) FROM revisions", [], |r| r.get(0))
            .optional()?
            .flatten();
        Ok(id.unwrap_or(0) as u64)
    }

    /// Snapshot of all currently-commissioned blueprints, keyed by name.
    fn active_blueprints(&self) -> Result<BTreeMap<String, Blueprint>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name, body FROM blueprints")?;
        let rows = stmt.query_map([], |r| {
            let name: String = r.get(0)?;
            let body: String = r.get(1)?;
            Ok((name, body))
        })?;
        let mut out = BTreeMap::new();
        for r in rows {
            let (name, body) = r?;
            let bp: Blueprint = serde_json::from_str(&body)
                .with_context(|| format!("decode blueprint {name}"))?;
            out.insert(name, bp);
        }
        Ok(out)
    }

    pub fn list_blueprints(&self) -> Result<Vec<Blueprint>> {
        Ok(self.active_blueprints()?.into_values().collect())
    }

    pub fn current_state(&self) -> Result<State> {
        Ok(State::resolve(&self.active_blueprints()?))
    }

    /// Commission (insert or replace) a blueprint, recompute state,
    /// append a revision. Returns the new revision.
    pub fn commission(&self, bp: Blueprint) -> Result<Revision> {
        let mut active = self.active_blueprints()?;
        let prior_state = State::resolve(&active);
        active.insert(bp.name.clone(), bp.clone());
        let new_state = State::resolve(&active);
        let actions = new_state.actions_from(&prior_state);

        {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO blueprints(name, body) VALUES(?1, ?2)
                 ON CONFLICT(name) DO UPDATE SET body = excluded.body",
                params![bp.name, serde_json::to_string(&bp)?],
            )?;
        }

        self.append_revision(
            RevisionKind::Commission,
            Some(bp.name.clone()),
            &actions,
            &new_state,
        )
    }

    /// Decommission a blueprint by name. Returns `None` if no such
    /// blueprint exists (the caller maps that to 404).
    pub fn decommission(&self, name: &str) -> Result<Option<Revision>> {
        let mut active = self.active_blueprints()?;
        if !active.contains_key(name) {
            return Ok(None);
        }
        let prior_state = State::resolve(&active);
        active.remove(name);
        let new_state = State::resolve(&active);
        let actions = new_state.actions_from(&prior_state);

        {
            let conn = self.conn.lock().unwrap();
            conn.execute("DELETE FROM blueprints WHERE name = ?1", params![name])?;
        }

        Ok(Some(self.append_revision(
            RevisionKind::Decommission,
            Some(name.to_string()),
            &actions,
            &new_state,
        )?))
    }

    fn append_revision(
        &self,
        kind: RevisionKind,
        blueprint: Option<String>,
        actions: &[Action],
        state: &State,
    ) -> Result<Revision> {
        let now = Utc::now();
        let actions_json = serde_json::to_string(actions)?;
        let state_json = serde_json::to_string(state)?;
        let kind_str = match kind {
            RevisionKind::Init => "init",
            RevisionKind::Commission => "commission",
            RevisionKind::Decommission => "decommission",
        };
        let id = {
            let conn = self.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO revisions(at, kind, blueprint, actions, state)
                 VALUES(?1, ?2, ?3, ?4, ?5)",
                params![now.to_rfc3339(), kind_str, blueprint, actions_json, state_json],
            )?;
            conn.last_insert_rowid() as u64
        };
        Ok(Revision {
            id,
            at: now,
            kind,
            blueprint,
            actions: actions.to_vec(),
            state: state.clone(),
        })
    }

    pub fn list_revisions(&self) -> Result<Vec<Revision>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, at, kind, blueprint, actions, state
             FROM revisions ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], row_to_revision)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Into::into)
    }

    pub fn get_revision(&self, id: u64) -> Result<Option<Revision>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, at, kind, blueprint, actions, state
             FROM revisions WHERE id = ?1",
            params![id as i64],
            row_to_revision,
        )
        .optional()
        .map_err(Into::into)
    }
}

fn row_to_revision(r: &rusqlite::Row) -> rusqlite::Result<Revision> {
    let id: i64 = r.get(0)?;
    let at: String = r.get(1)?;
    let kind: String = r.get(2)?;
    let blueprint: Option<String> = r.get(3)?;
    let actions: String = r.get(4)?;
    let state: String = r.get(5)?;

    let at = chrono::DateTime::parse_from_rfc3339(&at)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?
        .with_timezone(&Utc);

    let kind = match kind.as_str() {
        "init" => RevisionKind::Init,
        "commission" => RevisionKind::Commission,
        "decommission" => RevisionKind::Decommission,
        other => {
            return Err(rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                format!("unknown revision kind {other}").into(),
            ))
        }
    };

    let actions: Vec<Action> = serde_json::from_str(&actions)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e)))?;
    let state: State = serde_json::from_str(&state)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e)))?;

    Ok(Revision {
        id: id as u64,
        at,
        kind,
        blueprint,
        actions,
        state,
    })
}
