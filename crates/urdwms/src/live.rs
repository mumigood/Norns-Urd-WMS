//! Track commits from other connections without resetting in-progress forms.
use crate::error::Res;
use rusqlite::Connection;
pub struct ChangeTracker {
    version: i64,
}
impl ChangeTracker {
    pub fn new(conn: &Connection) -> Res<Self> {
        Ok(Self { version: conn.pragma_query_value(None, "data_version", |r| r.get(0))? })
    }
    pub fn changed(&mut self, conn: &Connection) -> Res<bool> {
        let version = conn.pragma_query_value(None, "data_version", |r| r.get(0))?;
        let changed = self.version != version;
        self.version = version;
        Ok(changed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notices_external_commits_but_not_own_writes_or_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.sqlite");
        let a = crate::db::open(&path).unwrap();
        a.execute_batch("CREATE TABLE data(value TEXT)").unwrap();
        let b = crate::db::open(&path).unwrap();
        let mut watch = ChangeTracker::new(&a).unwrap();
        a.execute("INSERT INTO data VALUES ('own')", []).unwrap();
        assert!(!watch.changed(&a).unwrap());
        b.execute("INSERT INTO data VALUES ('remote')", []).unwrap();
        assert!(watch.changed(&a).unwrap());
        assert!(!watch.changed(&a).unwrap());
        b.execute_batch("BEGIN; INSERT INTO data VALUES ('rollback'); ROLLBACK;").unwrap();
        assert!(!watch.changed(&a).unwrap());
    }
}
