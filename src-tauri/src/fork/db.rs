//! Fork schema: `fork_*` tables and one-time data repairs, versioned apart from
//! upstream.
//!
//! Upstream owns `PRAGMA user_version` (its `MIGRATIONS` list). Sharing that
//! sequence would collide the day upstream ships its own 0016, so the fork keeps
//! its version as a value row in the upstream `settings` table under
//! [`VERSION_KEY`]. Each step and its version bump commit together, exactly like
//! `db::migrate`: a failing statement rolls the whole step back and the next
//! start retries it from scratch.

use crate::error::Result;
use rusqlite::{Connection, OptionalExtension};

/// Settings row that carries the fork schema version.
pub const VERSION_KEY: &str = "fork_schema_version";

/// Fork migrations, in order. Index + 1 is the version they bring the schema to.
/// Never reorder, never edit a shipped file: add the next one.
pub const FORK_MIGRATIONS: &[&str] = &[
    include_str!("migrations/f0001_gmail_provider.sql"),
    include_str!("migrations/f0002_important_role.sql"),
];

pub fn current_version(conn: &Connection) -> rusqlite::Result<i64> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [VERSION_KEY],
            |r| r.get(0),
        )
        .optional()?;
    Ok(stored.and_then(|v| v.parse().ok()).unwrap_or(0))
}

/// Apply every fork migration past the stored version. Idempotent.
pub fn migrate(conn: &mut Connection) -> Result<()> {
    migrate_with(conn, FORK_MIGRATIONS)
}

fn migrate_with(conn: &mut Connection, migrations: &[&str]) -> Result<()> {
    let version = current_version(conn)?;
    for (i, sql) in migrations.iter().enumerate() {
        let target = (i + 1) as i64;
        if version < target {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![VERSION_KEY, target.to_string()],
            )?;
            tx.commit()?;
            tracing::info!(migration = target, "applied fork migration");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare connection with only the upstream `settings` table, which is all
    /// the runner itself needs.
    fn upstream_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);")
            .unwrap();
        conn
    }

    #[test]
    fn shipped_migrations_apply_cleanly_and_rerun_is_a_noop() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            assert_eq!(
                current_version(conn)?,
                FORK_MIGRATIONS.len() as i64,
                "Db::init must run the fork runner"
            );
            Ok(())
        })
        .unwrap();
        db.with(|conn| {
            migrate(conn).unwrap();
            assert_eq!(current_version(conn)?, FORK_MIGRATIONS.len() as i64);
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn steps_apply_in_order_and_bump_the_version_row() {
        let mut conn = upstream_conn();
        let steps = &[
            "CREATE TABLE fork_one (x INTEGER);",
            "CREATE TABLE fork_two (y INTEGER); INSERT INTO fork_one VALUES (1);",
        ];
        migrate_with(&mut conn, steps).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 2);
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM fork_one", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
        // Re-run: nothing happens, no "table already exists".
        migrate_with(&mut conn, steps).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 2);
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM fork_one", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1, "a re-run must not re-insert");
    }

    #[test]
    fn only_new_steps_run_on_an_upgraded_database() {
        let mut conn = upstream_conn();
        migrate_with(&mut conn, &["CREATE TABLE fork_one (x INTEGER);"]).unwrap();
        migrate_with(
            &mut conn,
            &[
                "CREATE TABLE fork_one (x INTEGER);",
                "CREATE TABLE fork_two (y INTEGER);",
            ],
        )
        .unwrap();
        assert_eq!(current_version(&conn).unwrap(), 2);
    }

    #[test]
    fn failed_migration_rolls_back_entirely() {
        let mut conn = upstream_conn();
        let bad = &["CREATE TABLE fork_half (x INTEGER); THIS IS NOT SQL;"];
        assert!(migrate_with(&mut conn, bad).is_err());
        assert_eq!(current_version(&conn).unwrap(), 0);
        let leftover: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'fork_half'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(leftover, 0);
    }

    #[test]
    fn upstream_user_version_is_untouched() {
        let mut conn = upstream_conn();
        let before: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        migrate_with(&mut conn, &["CREATE TABLE fork_one (x INTEGER);"]).unwrap();
        let after: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after);
    }
}

#[cfg(test)]
mod repair_tests {
    use super::*;

    /// Run the shipped fork migrations again over rows shaped like the live
    /// database before the fork: a hand-added Gmail account and Important
    /// mapped to starred.
    #[test]
    fn f0001_and_f0002_repair_live_rows() {
        let db = crate::db::Db::open_in_memory().unwrap();
        db.with(|conn| {
            conn.execute_batch(
                "INSERT INTO accounts (id, email, provider, imap_host, smtp_host, created_at)
                   VALUES ('a1','p@autospark.ai','custom','imap.gmail.com','smtp.gmail.com',0),
                          ('a2','q@example.com','custom','imap.example.com','smtp.example.com',0);
                 INSERT INTO folders (id, account_id, imap_name, role, display_name, sort_order)
                   VALUES (1,'a1','[Gmail]/Important','starred','Important',1),
                          (2,'a1','[Gmail]/Starred','starred','Starred',1),
                          (3,'a2','Important','starred','Important',1);
                 DELETE FROM settings WHERE key = 'fork_schema_version';",
            )?;
            migrate(conn).unwrap();
            let provider = |id: &str| -> String {
                conn.query_row("SELECT provider FROM accounts WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .unwrap()
            };
            assert_eq!(provider("a1"), "gmail");
            assert_eq!(provider("a2"), "custom", "a non-Gmail host is left alone");
            let role = |id: i64| -> String {
                conn.query_row("SELECT role FROM folders WHERE id = ?1", [id], |r| r.get(0))
                    .unwrap()
            };
            assert_eq!(role(1), "important");
            assert_eq!(role(2), "starred");
            assert_eq!(role(3), "important");
            Ok(())
        })
        .unwrap();
    }
}
