//! Claim state persistence.
//!
//! SQLite is the disk image of the in-memory ClaimState map. The agent
//! authoritatively runs from memory; this exists so that a crash or reboot
//! doesn't lose the fact that we installed caddy, or what the prior file
//! content was for rollback.
//!
//! Write discipline: the reconciler calls `put` immediately after any
//! mutation to ClaimState — in particular, before invoking a provider's
//! apply (so first-touch preexisting-capture is durable) and again after.

use anyhow::{Context, Result};
use golem_types::{ClaimId, ClaimState};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;
use tracing::info;

pub struct Store {
    conn: Mutex<Connection>,
}

/// Schema migrations are append-only. Each integer corresponds to one ALTER
/// or one-shot data fix-up; on Store::open we replay every migration whose
/// version is greater than the highest in `schema_version`.
///
/// Migration history:
///   1 — Provider trait split (DESIGN.md §6). Synthesize `captured=true` for
///       any pre-trait-split row whose existing fields prove a capture had
///       run under the old in-place logic. Without this, an in-place upgrade
///       would re-capture against an already-mutated OS and break honest
///       cleanup. See REVIEW finding [P1] for the full failure mode.
const MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        r#"
        UPDATE claim_state
           SET json = json_set(json, '$.captured', json('true'))
         WHERE COALESCE(json_extract(json, '$.captured'), 0) = 0
           AND (
                json_extract(json, '$.last_applied')      IS NOT NULL OR
                json_extract(json, '$.preexisting')        = 1          OR
                json_extract(json, '$.installed_by_us')    = 1          OR
                json_extract(json, '$.backup.existed')     = 1
           );
        "#,
    ),
];

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous  = NORMAL;

            CREATE TABLE IF NOT EXISTS claim_state (
                id_kind    TEXT NOT NULL,
                id_key     TEXT NOT NULL,
                json       TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (id_kind, id_key)
            );

            CREATE TABLE IF NOT EXISTS bundle (
                version     INTEGER PRIMARY KEY,
                signer      TEXT NOT NULL,
                received_at INTEGER NOT NULL,
                body        BLOB NOT NULL
            );

            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at INTEGER NOT NULL
            );
            "#,
        )?;
        Self::run_migrations(&conn).context("schema migration")?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    fn run_migrations(conn: &Connection) -> Result<()> {
        let current: i64 = conn
            .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_version", [], |r| r.get(0))
            .unwrap_or(0);
        let now = chrono::Utc::now().timestamp();
        for (version, sql) in MIGRATIONS {
            if *version <= current {
                continue;
            }
            info!("applying schema migration {version}");
            conn.execute_batch(sql)
                .with_context(|| format!("migration {version}"))?;
            conn.execute(
                "INSERT INTO schema_version (version, applied_at) VALUES (?1, ?2)",
                params![version, now],
            )?;
        }
        Ok(())
    }

    pub fn put(&self, state: &ClaimState) -> Result<()> {
        let json = serde_json::to_string(state)?;
        let now  = chrono::Utc::now().timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO claim_state (id_kind, id_key, json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id_kind, id_key) DO UPDATE SET
                json       = excluded.json,
                updated_at = excluded.updated_at",
            params![state.id.kind.as_str(), &state.id.key, json, now],
        )?;
        Ok(())
    }

    pub fn forget(&self, id: &ClaimId) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM claim_state WHERE id_kind = ?1 AND id_key = ?2",
            params![id.kind.as_str(), &id.key],
        )?;
        Ok(())
    }

    pub fn load_all(&self) -> Result<Vec<ClaimState>> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn.prepare("SELECT json FROM claim_state")?;
        let rows  = st.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(serde_json::from_str(&r?)?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a "legacy" claim_state JSON row of the shape the pre-trait-split
    /// agent wrote. Crucially, no `captured` field — that's the field the
    /// migration is responsible for backfilling.
    fn legacy_row(
        id_kind: &str,
        id_key: &str,
        installed_by_us: bool,
        preexisting: bool,
        last_applied_set: bool,
    ) -> String {
        let last_applied = if last_applied_set {
            "\"2024-01-01T00:00:00Z\""
        } else {
            "null"
        };
        format!(
            r#"{{
                "id": {{ "kind": "{id_kind}", "key": "{id_key}" }},
                "preexisting": {preexisting},
                "installed_by_us": {installed_by_us},
                "backup": {{ "existed": {preexisting} }},
                "last_applied": {last_applied},
                "last_health": null,
                "content_hash": null,
                "last_spec": null
            }}"#
        )
    }

    /// Insert a raw JSON row directly into a freshly-created claim_state
    /// table, bypassing Store::put (which would write the new schema shape).
    fn seed_legacy_db(path: &std::path::Path, rows: &[(&str, &str, String)]) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE claim_state (
                id_kind    TEXT NOT NULL,
                id_key     TEXT NOT NULL,
                json       TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY (id_kind, id_key)
            );
            "#,
        )
        .unwrap();
        for (kind, key, json) in rows {
            conn.execute(
                "INSERT INTO claim_state (id_kind, id_key, json, updated_at) VALUES (?1, ?2, ?3, 0)",
                params![kind, key, json],
            )
            .unwrap();
        }
    }

    fn captured_flag(path: &std::path::Path, kind: &str, key: &str) -> bool {
        let conn = Connection::open(path).unwrap();
        let json: String = conn
            .query_row(
                "SELECT json FROM claim_state WHERE id_kind = ?1 AND id_key = ?2",
                params![kind, key],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        v.get("captured").and_then(|x| x.as_bool()).unwrap_or(false)
    }

    fn schema_max_version(path: &std::path::Path) -> i64 {
        let conn = Connection::open(path).unwrap();
        conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_version",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Migration on a fresh DB: tables created, schema_version=1, no rows touched.
    #[test]
    fn migration_on_fresh_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        let _store = Store::open(&path).unwrap();
        assert_eq!(schema_max_version(&path), 1);
    }

    /// Legacy row with last_applied=Some: migration must set captured=true,
    /// because the old in-place capture had honestly recorded preexisting/backup.
    #[test]
    fn migration_marks_applied_rows_as_captured() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        seed_legacy_db(
            &path,
            &[(
                "apt_package",
                "caddy",
                legacy_row("apt_package", "caddy", true, false, true),
            )],
        );
        let _store = Store::open(&path).unwrap();
        assert!(
            captured_flag(&path, "apt_package", "caddy"),
            "row with last_applied=Some must be marked captured=true"
        );
        assert_eq!(schema_max_version(&path), 1);
    }

    /// Legacy row with installed_by_us=true but last_applied=None: still must
    /// be marked captured=true, because the old apply set installed_by_us
    /// only after a successful mutation, which means capture had run.
    #[test]
    fn migration_marks_installed_rows_as_captured() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        seed_legacy_db(
            &path,
            &[(
                "file",
                "/etc/foo",
                legacy_row("file", "/etc/foo", true, false, false),
            )],
        );
        let _store = Store::open(&path).unwrap();
        assert!(captured_flag(&path, "file", "/etc/foo"));
    }

    /// Legacy row with preexisting=true: a capture observation was recorded
    /// even if mutate didn't complete. Mark captured=true.
    #[test]
    fn migration_marks_preexisting_rows_as_captured() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        seed_legacy_db(
            &path,
            &[(
                "systemd_unit",
                "foo.service",
                legacy_row("systemd_unit", "foo.service", false, true, false),
            )],
        );
        let _store = Store::open(&path).unwrap();
        assert!(captured_flag(&path, "systemd_unit", "foo.service"));
    }

    /// Legacy row that's truly fresh (no last_applied, no installed_by_us, no
    /// preexisting, no backup): migration must NOT mark captured=true. The
    /// reconciler will do an honest fresh capture next tick.
    #[test]
    fn migration_leaves_unobserved_rows_uncaptured() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        seed_legacy_db(
            &path,
            &[(
                "file",
                "/etc/never-touched",
                legacy_row("file", "/etc/never-touched", false, false, false),
            )],
        );
        let _store = Store::open(&path).unwrap();
        assert!(
            !captured_flag(&path, "file", "/etc/never-touched"),
            "row with no capture evidence must stay captured=false"
        );
    }

    /// Idempotency: running Store::open twice on the same DB does not
    /// reapply the migration.
    #[test]
    fn migration_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.db");
        seed_legacy_db(
            &path,
            &[(
                "apt_package",
                "caddy",
                legacy_row("apt_package", "caddy", true, false, true),
            )],
        );
        let _ = Store::open(&path).unwrap();
        let _ = Store::open(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM schema_version WHERE version = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "migration 1 should be recorded exactly once");
    }
}
