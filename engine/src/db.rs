//! Ouverture SQLite en lecture seule sur copie (avec sidecars -wal / -shm).

use rusqlite::{Connection, OpenFlags};
use std::collections::HashSet;
use std::path::Path;
use tempfile::TempDir;

pub struct ReadOnlyDb {
    pub conn: Connection,
    _tmp: TempDir,
}

impl ReadOnlyDb {
    /// Copie la base et ses sidecars dans un dossier temporaire, puis ouvre la
    /// copie en lecture seule. L'original n'est jamais modifie.
    pub fn open(path: &Path) -> Result<ReadOnlyDb, String> {
        let tmp = TempDir::new().map_err(|e| e.to_string())?;
        let name = path
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_else(|| "db".into());
        let dst = tmp.path().join(&name);
        std::fs::copy(path, &dst).map_err(|e| e.to_string())?;
        for suffix in ["-wal", "-shm"] {
            let side = append_suffix(path, suffix);
            if side.exists() {
                let dst_side = append_suffix(&dst, suffix);
                let _ = std::fs::copy(&side, &dst_side);
            }
        }
        let conn = Connection::open_with_flags(&dst, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        Ok(ReadOnlyDb { conn, _tmp: tmp })
    }

    pub fn table_names(&self) -> HashSet<String> {
        let mut set = HashSet::new();
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(0)) {
                for r in rows.flatten() {
                    set.insert(r);
                }
            }
        }
        set
    }

    pub fn columns(&self, table: &str) -> HashSet<String> {
        let mut set = HashSet::new();
        let sql = format!("PRAGMA table_info(\"{}\")", table.replace('"', ""));
        if let Ok(mut stmt) = self.conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) {
                for r in rows.flatten() {
                    set.insert(r);
                }
            }
        }
        set
    }
}

fn append_suffix(path: &Path, suffix: &str) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(suffix);
    std::path::PathBuf::from(s)
}

/// Lit une table dont certaines colonnes peuvent manquer : renvoie, pour chaque
/// nom demande, l'expression SQL a utiliser (le nom si present, sinon NULL).
pub fn pick<'a>(cols: &HashSet<String>, name: &'a str) -> String {
    if cols.contains(name) {
        format!("\"{}\"", name)
    } else {
        "NULL".to_string()
    }
}
