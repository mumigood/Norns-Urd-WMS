//! Versioned, backed-up upgrades of existing desktop databases.

use crate::{bail, db, error::Res};
use rusqlite::{
    Connection, OpenFlags, Transaction, TransactionBehavior,
    backup::{Backup, StepResult},
};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const CURRENT_VERSION: i64 = 1;
const INDEXES: &str =
    "CREATE UNIQUE INDEX IF NOT EXISTS idx_item_model\n    ON inventory_item(model) WHERE model IS NOT NULL;";

fn version(conn: &Connection) -> Res<i64> {
    Ok(conn.pragma_query_value(None, "user_version", |r| r.get(0))?)
}
fn tables(conn: &Connection) -> Res<Vec<String>> {
    Ok(conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")?
        .query_map([], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}
fn columns(conn: &Connection, table: &str) -> Res<Vec<String>> {
    // Only schema-owned table names are passed here.
    Ok(conn
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |r| r.get(1))?
        .collect::<rusqlite::Result<_>>()?)
}
fn integrity(conn: &Connection) -> Res<()> {
    let mut statement = conn.prepare("PRAGMA integrity_check")?;
    let messages = statement.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    if messages.len() != 1 || messages[0] != "ok" {
        bail!("資料庫完整性檢查失敗，停止升級：{}", messages.join("；"));
    }
    Ok(())
}

/// Upgrade under a SQLite writer reservation so two launchers cannot migrate
/// the same version, and the backup is exactly the state being upgraded.
/// Ordinary HTTP request connections only call db::open, never this function.
pub fn upgrade(conn: &Connection, path: Option<&Path>) -> Res<()> {
    let found = version(conn)?;
    if found > CURRENT_VERSION {
        bail!("資料庫版本 {found} 比本程式支援的 {CURRENT_VERSION} 新，請使用較新版程式；未變更資料");
    }
    if found == CURRENT_VERSION {
        return Ok(());
    }
    let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    // Another process may have completed the migration while we waited.
    let found = version(&tx)?;
    if found > CURRENT_VERSION {
        bail!("資料庫版本 {found} 不受支援，停止升級");
    }
    if found == CURRENT_VERSION {
        tx.commit()?;
        return Ok(());
    }
    integrity(&tx)?;
    let existing = tables(&tx)?;
    if !existing.is_empty() {
        if !existing.iter().any(|name| name == "inventory_item") {
            bail!("這不是可辨識的 UrdWMS 資料庫，停止升級；未變更資料");
        }
        if let Some(path) = path {
            let saved = backup(path, found)?;
            let message = format!("升級前備份：{}", saved.display());
            eprintln!("{message}");
            crate::applog::info(message);
        }
    }
    apply(&tx)?;
    tx.pragma_update(None, "user_version", CURRENT_VERSION)?;
    tx.commit()?;
    Ok(())
}

fn apply(conn: &Connection) -> Res<()> {
    // CREATE INDEX must follow ALTER TABLE: early releases can lack model.
    conn.execute_batch(&db::SCHEMA.replace(INDEXES, ""))?;
    for (table, column, ddl) in db::ADDED_COLUMNS {
        if !columns(conn, table)?.iter().any(|present| present == column) {
            conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {ddl}"), [])?;
        }
    }
    // Refuse unknown/incomplete legacy schemas instead of tagging an unusable
    // database as current. New tables can be created; existing data is never
    // renamed, dropped, or filled with guessed required values.
    let reference = Connection::open_in_memory()?;
    reference.execute_batch(db::SCHEMA)?;
    for table in tables(&reference)? {
        let actual = columns(conn, &table)?;
        for expected in columns(&reference, &table)? {
            if !actual.contains(&expected) {
                bail!("不支援的舊資料庫結構：{table} 缺少 {expected}；已取消升級");
            }
        }
    }
    conn.execute_batch(INDEXES)?;
    Ok(())
}

fn backup(path: &Path, old_version: i64) -> Res<PathBuf> {
    let dir = path.parent().unwrap_or_else(|| Path::new(".")).join("backups");
    std::fs::create_dir_all(&dir)?;
    let mut random = [0u8; 8];
    getrandom::fill(&mut random).map_err(|e| crate::error::AppError(format!("無法建立備份識別碼：{e}")))?;
    let name =
        format!("urdwms-v{old_version}-{}-{}.db", chrono::Utc::now().format("%Y%m%dT%H%M%S%fZ"), hex::encode(random));
    let final_path = dir.join(&name);
    let partial = dir.join(format!("{name}.partial"));
    // Reserve an exclusive filename; never overwrite an earlier backup.
    let file = std::fs::OpenOptions::new().write(true).create_new(true).open(&partial)?;
    drop(file);
    let result = (|| -> Res<()> {
        let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        source.busy_timeout(Duration::from_secs(5))?;
        let mut destination = Connection::open(&partial)?;
        {
            let copy = Backup::new(&source, &mut destination)?;
            let deadline = Instant::now() + Duration::from_secs(60);
            loop {
                match copy.step(128)? {
                    StepResult::Done => break,
                    StepResult::More => {}
                    StepResult::Busy | StepResult::Locked => std::thread::sleep(Duration::from_millis(10)),
                    _ => bail!("備份狀態無法辨識，停止升級"),
                }
                if Instant::now() >= deadline {
                    bail!("資料庫備份逾時，停止升級；原資料未變更");
                }
            }
        }
        integrity(&destination)?;
        destination.close().map_err(|(_, e)| e)?;
        std::fs::OpenOptions::new().write(true).open(&partial)?.sync_all()?;
        std::fs::rename(&partial, &final_path)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use crate::{db, service::Store};
    use rusqlite::Connection;
    use std::path::{Path, PathBuf};

    fn backups(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir.join("backups"))
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "db"))
            .collect()
    }
    fn version(c: &Connection) -> i64 {
        c.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap()
    }
    fn columns(c: &Connection, table: &str) -> Vec<String> {
        c.prepare(&format!("PRAGMA table_info({table})"))
            .unwrap()
            .query_map([], |r| r.get(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap()
    }
    fn legacy(dir: &Path) -> Connection {
        let conn = db::open(&dir.join("urdwms.db")).unwrap();
        conn.execute_batch(db::SCHEMA).unwrap();
        crate::seed::demo(&conn).unwrap();
        conn.execute(
            "INSERT INTO app_setting(key,value,updated_at) VALUES ('migration-proof','\"kept\"','2026-01-01')",
            [],
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("uploads")).unwrap();
        std::fs::write(dir.join("uploads/old.jpg"), b"original-image").unwrap();
        conn
    }
    #[test]
    fn legacy_desktop_data_and_committed_wal_are_backed_up_before_migration() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path();
        let old = legacy(dir);
        old.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
        old.execute(
            "UPDATE inventory_item SET name='committed WAL value' WHERE id=(SELECT MIN(id) FROM inventory_item)",
            [],
        )
        .unwrap();
        let hash: String =
            old.query_row("SELECT password_hash FROM app_user WHERE username='peter'", [], |r| r.get(0)).unwrap();
        assert!(dir.join("urdwms.db-wal").exists());
        let store = Store::open(dir).unwrap();
        assert_eq!(version(&store.conn), 1);
        let files = backups(dir);
        assert_eq!(files.len(), 1);
        let snapshot = Connection::open(&files[0]).unwrap();
        assert_eq!(version(&snapshot), 0);
        let name: String =
            snapshot.query_row("SELECT name FROM inventory_item ORDER BY id LIMIT 1", [], |r| r.get(0)).unwrap();
        assert_eq!(name, "committed WAL value");
        let current: String = store
            .conn
            .query_row("SELECT password_hash FROM app_user WHERE username='peter'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hash, current);
        let setting: String = store
            .conn
            .query_row("SELECT value FROM app_setting WHERE key='migration-proof'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(setting, "\"kept\"");
        assert_eq!(std::fs::read(dir.join("uploads/old.jpg")).unwrap(), b"original-image");
        drop(store);
        Store::open(dir).unwrap();
        assert_eq!(backups(dir).len(), 1);
    }
    #[test]
    fn historical_missing_columns_are_added_before_indexes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("urdwms.db");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("CREATE TABLE inventory_item(id INTEGER PRIMARY KEY,name TEXT NOT NULL,spec TEXT,unit TEXT NOT NULL DEFAULT '箱',shelf_life_days INTEGER,safety_stock INTEGER NOT NULL DEFAULT 0); INSERT INTO inventory_item(id,name) VALUES (42,'old item');
        CREATE TABLE inventory_lot(id INTEGER PRIMARY KEY,item_id INTEGER NOT NULL,receipt_date TEXT NOT NULL,manufacture_date TEXT,supplier_lot_code TEXT,qty_on_hand INTEGER NOT NULL,created_at TEXT NOT NULL,created_by TEXT NOT NULL);
        INSERT INTO inventory_lot VALUES(9,42,'2026-01-01',NULL,NULL,7,'2026-01-01','old-user');").unwrap();
        drop(old);
        let store = Store::open(temp.path()).unwrap();
        assert_eq!(version(&store.conn), 1);
        assert!(columns(&store.conn, "inventory_item").contains(&"model".into()));
        assert!(columns(&store.conn, "inventory_lot").contains(&"qty_received".into()));
        let items = crate::service::items(&store.conn, crate::service::ItemOrder::Sort).unwrap();
        assert_eq!(items[0].id, 42);
        assert_eq!(items[0].on_hand, 7);
    }
    #[test]
    fn failed_index_creation_rolls_back_all_schema_changes() {
        let temp = tempfile::tempdir().unwrap();
        let old = Connection::open(temp.path().join("urdwms.db")).unwrap();
        old.execute_batch("CREATE TABLE inventory_item(id INTEGER PRIMARY KEY,name TEXT NOT NULL,model TEXT,spec TEXT,unit TEXT NOT NULL DEFAULT '箱',shelf_life_days INTEGER,safety_stock INTEGER NOT NULL DEFAULT 0); INSERT INTO inventory_item(id,name,model) VALUES(1,'one','duplicate'),(2,'two','duplicate');").unwrap();
        assert!(Store::open(temp.path()).is_err());
        assert_eq!(version(&old), 0);
        assert!(!columns(&old, "inventory_item").contains(&"meters_per_box".into()));
        let created: i64 =
            old.query_row("SELECT COUNT(*) FROM sqlite_master WHERE name='app_user'", [], |r| r.get(0)).unwrap();
        assert_eq!(created, 0);
        assert_eq!(backups(temp.path()).len(), 1);
    }
    #[test]
    fn backup_failure_and_future_version_leave_existing_data_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let old = legacy(temp.path());
        std::fs::write(temp.path().join("backups"), b"not a directory").unwrap();
        assert!(Store::open(temp.path()).is_err());
        assert_eq!(version(&old), 0);
        old.pragma_update(None, "user_version", 99).unwrap();
        assert!(Store::open(temp.path()).is_err());
        assert_eq!(version(&old), 99);
    }
    #[test]
    fn corrupt_database_is_not_replaced_and_new_database_is_versioned() {
        let temp = tempfile::tempdir().unwrap();
        let bytes = b"not a SQLite database";
        std::fs::write(temp.path().join("urdwms.db"), bytes).unwrap();
        assert!(Store::open(temp.path()).is_err());
        assert_eq!(std::fs::read(temp.path().join("urdwms.db")).unwrap(), bytes);
        let fresh = tempfile::tempdir().unwrap();
        let store = Store::open(fresh.path()).unwrap();
        assert_eq!(version(&store.conn), 1);
        assert!(backups(fresh.path()).is_empty());
    }
}
