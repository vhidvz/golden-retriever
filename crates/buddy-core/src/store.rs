//! Buddy's memory on disk: one SQLite file, versioned, backed up daily, and
//! recoverable if it is ever damaged.

use crate::behavior::Record;
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

/// Each entry upgrades the schema by one version; `PRAGMA user_version` records how many have run.
const MIGRATIONS: &[&str] = &[
    // v1: current state as named JSON documents, plus an append-only journal of what happened.
    "CREATE TABLE state (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at INTEGER NOT NULL);
     CREATE TABLE journal (id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, kind TEXT NOT NULL, data TEXT NOT NULL);
     CREATE INDEX journal_ts ON journal (ts);",
    // v2: what he did, when and why. Feeds his memory of recent behaviour and the "why" view.
    "CREATE TABLE behaviour (id INTEGER PRIMARY KEY, ts REAL NOT NULL, activity TEXT NOT NULL, variant INTEGER NOT NULL,
                             secs REAL NOT NULL, intensity REAL NOT NULL, cause TEXT NOT NULL, x_frac REAL NOT NULL);",
    // v3: things the user told him to remember, in their own words.
    "CREATE TABLE memories (id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, text TEXT NOT NULL, language TEXT NOT NULL, source TEXT NOT NULL);",
];

/// Rows of behaviour kept; older ones are pruned.
const BEHAVIOUR_KEPT: i64 = 50_000;

const BACKUPS_KEPT: usize = 7;

pub struct Store {
    conn: Connection,
    path: PathBuf,
    /// Set when the database had to be replaced on open; the daemon journals it.
    pub recovery_note: Option<String>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let mut recovery_note = None;
        if path.exists() && !is_healthy(path) {
            recovery_note = Some(recover(path)?);
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let store = Self { conn, path: path.to_path_buf(), recovery_note };
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> Result<usize> {
        Ok(self.conn.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))? as usize)
    }

    fn migrate(&self) -> Result<()> {
        let current = self.schema_version()?;
        if current > MIGRATIONS.len() {
            bail!("{} was written by a newer Buddy (schema {current}, this build knows {})", self.path.display(), MIGRATIONS.len());
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
            self.conn.execute_batch(&format!("BEGIN; {sql} PRAGMA user_version = {}; COMMIT;", i + 1))?;
        }
        Ok(())
    }

    pub fn load<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        let mut stmt = self.conn.prepare("SELECT value FROM state WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        match rows.next()? {
            Some(row) => Ok(Some(serde_json::from_str(&row.get::<_, String>(0)?).with_context(|| format!("decoding state '{key}'"))?)),
            None => Ok(None),
        }
    }

    pub fn save<T: Serialize>(&self, key: &str, value: &T, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO state (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, serde_json::to_string(value)?, now],
        )?;
        Ok(())
    }

    pub fn journal(&self, ts: i64, kind: &str, data: &Value) -> Result<()> {
        self.conn.execute("INSERT INTO journal (ts, kind, data) VALUES (?1, ?2, ?3)", params![ts, kind, data.to_string()])?;
        Ok(())
    }

    pub fn log_behaviour(&self, r: &Record) -> Result<()> {
        self.conn.execute(
            "INSERT INTO behaviour (ts, activity, variant, secs, intensity, cause, x_frac) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![r.ts, r.activity.name(), r.variant, r.secs, r.intensity, r.cause, r.x_frac],
        )?;
        let id = self.conn.last_insert_rowid();
        if id % 1000 == 0 {
            self.conn.execute("DELETE FROM behaviour WHERE id <= ?1", params![id - BEHAVIOUR_KEPT])?;
        }
        Ok(())
    }

    /// The most recent behaviour, oldest first.
    pub fn recent_behaviour(&self, limit: usize) -> Result<Vec<Record>> {
        let mut stmt = self.conn.prepare("SELECT ts, activity, variant, secs, intensity, cause, x_frac FROM behaviour ORDER BY id DESC LIMIT ?1")?;
        let mut rows = stmt.query(params![limit as i64])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            // An activity this build no longer knows is skipped rather than failing the load.
            if let Ok(activity) = serde_json::from_value(Value::String(r.get(1)?)) {
                out.push(Record {
                    ts: r.get(0)?,
                    activity,
                    variant: r.get(2)?,
                    secs: r.get(3)?,
                    intensity: r.get(4)?,
                    cause: r.get(5)?,
                    x_frac: r.get(6)?,
                });
            }
        }
        out.reverse();
        Ok(out)
    }

    /// Keeps something the user said to remember. `source` says how he came by it ("told").
    pub fn remember(&self, ts: i64, text: &str, language: &str, source: &str) -> Result<i64> {
        self.conn.execute("INSERT INTO memories (ts, text, language, source) VALUES (?1, ?2, ?3, ?4)", params![ts, text, language, source])?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Everything he was told to remember, oldest first.
    pub fn memories(&self) -> Result<Vec<Value>> {
        let mut stmt = self.conn.prepare("SELECT id, ts, text, language, source FROM memories ORDER BY id")?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(json!({ "id": r.get::<_, i64>(0)?, "ts": r.get::<_, i64>(1)?, "text": r.get::<_, String>(2)?, "language": r.get::<_, String>(3)?, "source": r.get::<_, String>(4)? }));
        }
        Ok(out)
    }

    /// True if there was such a memory to forget.
    pub fn forget_memory(&self, id: i64) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM memories WHERE id = ?1", params![id])? > 0)
    }

    /// Newest first.
    pub fn recent_journal(&self, limit: usize) -> Result<Vec<Value>> {
        let mut stmt = self.conn.prepare("SELECT ts, kind, data FROM journal ORDER BY id DESC LIMIT ?1")?;
        let rows = stmt.query_map(params![limit as i64], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (ts, kind, data) = row?;
            out.push(json!({ "ts": ts, "kind": kind, "data": serde_json::from_str::<Value>(&data).unwrap_or(Value::Null) }));
        }
        Ok(out)
    }

    /// Everything Buddy knows, as one JSON document the user can read or keep.
    pub fn export(&self, now: i64) -> Result<Value> {
        let mut state = serde_json::Map::new();
        let mut stmt = self.conn.prepare("SELECT key, value FROM state ORDER BY key")?;
        for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (key, value) = row?;
            state.insert(key, serde_json::from_str(&value).unwrap_or(Value::Null));
        }
        let mut journal = self.recent_journal(usize::MAX >> 1)?;
        journal.reverse();
        Ok(json!({ "schema_version": self.schema_version()?, "exported_at": now, "state": state, "memories": self.memories()?, "journal": journal }))
    }

    /// Writes today's backup if there is none yet, and prunes old ones.
    pub fn backup_daily(&self, now: i64) -> Result<Option<PathBuf>> {
        let dir = backup_dir(&self.path);
        fs::create_dir_all(&dir)?;
        let (y, m, d) = civil_date(now);
        let target = dir.join(format!("buddy-{y:04}{m:02}{d:02}.db"));
        if target.exists() {
            return Ok(None);
        }
        self.conn.execute("VACUUM INTO ?1", params![target.to_string_lossy()])?;
        let mut backups = list_backups(&dir);
        while backups.len() > BACKUPS_KEPT {
            let _ = fs::remove_file(backups.remove(0));
        }
        Ok(Some(target))
    }
}

fn backup_dir(db: &Path) -> PathBuf {
    db.parent().unwrap_or(Path::new(".")).join("backups")
}

/// Oldest first; the date in the name makes lexical order chronological.
fn list_backups(dir: &Path) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("buddy-") && n.ends_with(".db")))
        .collect();
    found.sort();
    found
}

fn is_healthy(path: &Path) -> bool {
    Connection::open(path).and_then(|c| c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))).is_ok_and(|verdict| verdict == "ok")
}

/// Sets the damaged file aside and restores the newest healthy backup, if any.
fn recover(path: &Path) -> Result<String> {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let aside = path.with_extension(format!("db.damaged-{stamp}"));
    fs::rename(path, &aside).with_context(|| format!("setting aside {}", path.display()))?;
    for suffix in ["db-wal", "db-shm"] {
        let _ = fs::remove_file(path.with_extension(suffix));
    }
    for backup in list_backups(&backup_dir(path)).into_iter().rev() {
        if is_healthy(&backup) {
            fs::copy(&backup, path)?;
            return Ok(format!("database was damaged; restored {} (damaged copy kept as {})", backup.display(), aside.display()));
        }
    }
    Ok(format!("database was damaged and no backup existed; started fresh (damaged copy kept as {})", aside.display()))
}

/// Unix seconds to a UTC calendar date (Howard Hinnant's days-to-civil algorithm).
fn civil_date(unix: i64) -> (i64, i64, i64) {
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Identity, Life};

    fn temp_db(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("buddy-store-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir.join("buddy.db")
    }

    #[test]
    fn state_and_journal_survive_reopening() {
        let path = temp_db("reopen");
        let identity = Identity { name: "Buddy".into(), born_at: 1_700_000_000 };
        {
            let store = Store::open(&path).unwrap();
            store.save("identity", &identity, 1).unwrap();
            store.save("life", &Life { awake_secs: 12.5, sessions: 3, last_seen: 9, pets: 2 }, 1).unwrap();
            store.journal(5, "born", &json!({ "name": "Buddy" })).unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len());
        assert_eq!(store.load::<Identity>("identity").unwrap(), Some(identity));
        assert_eq!(store.load::<Life>("life").unwrap().unwrap().sessions, 3);
        assert_eq!(store.load::<Life>("missing").unwrap(), None);
        let export = store.export(10).unwrap();
        assert_eq!(export["journal"][0]["kind"], "born");
        assert_eq!(export["state"]["identity"]["name"], "Buddy");
    }

    #[test]
    fn a_phase_one_database_gains_the_behaviour_log() {
        let path = temp_db("upgrade");
        {
            // A database exactly as the first schema left it.
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let old = Connection::open(&path).unwrap();
            old.execute_batch(&format!("{} PRAGMA user_version = 1;", MIGRATIONS[0])).unwrap();
            old.execute("INSERT INTO state VALUES ('identity', '{\"name\":\"Buddy\",\"born_at\":7}', 1)", []).unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.schema_version().unwrap(), MIGRATIONS.len());
        assert_eq!(store.load::<Identity>("identity").unwrap().unwrap().born_at, 7, "nothing is lost in the upgrade");
        let record = Record {
            ts: 5.5,
            activity: crate::model::Activity::TailChase,
            variant: 1,
            secs: 6.0,
            intensity: 0.7,
            cause: "playful".into(),
            x_frac: 0.4,
        };
        store.log_behaviour(&record).unwrap();
        assert_eq!(store.recent_behaviour(10).unwrap(), vec![record]);
        let id = store.remember(9, "من شب‌ها کار می‌کنم", "fa", "told").unwrap();
        assert_eq!(store.memories().unwrap()[0]["text"], "من شب‌ها کار می‌کنم");
        assert!(store.forget_memory(id).unwrap() && !store.forget_memory(id).unwrap());
    }

    #[test]
    fn a_damaged_database_is_restored_from_backup() {
        let path = temp_db("recover");
        {
            let store = Store::open(&path).unwrap();
            store.save("identity", &Identity { name: "Buddy".into(), born_at: 7 }, 1).unwrap();
            assert!(store.backup_daily(1_794_000_000).unwrap().is_some());
            assert!(store.backup_daily(1_794_000_000).unwrap().is_none(), "one backup per day");
        }
        fs::write(&path, b"this is not a database").unwrap();
        let store = Store::open(&path).unwrap();
        assert!(store.recovery_note.as_deref().unwrap().contains("restored"));
        assert_eq!(store.load::<Identity>("identity").unwrap().unwrap().born_at, 7);
    }

    #[test]
    fn a_database_from_a_newer_build_is_refused() {
        let path = temp_db("newer");
        Store::open(&path).unwrap().conn.pragma_update(None, "user_version", 99).unwrap();
        assert!(Store::open(&path).is_err());
    }

    #[test]
    fn calendar_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(1_794_441_600), (2026, 11, 12));
    }
}
