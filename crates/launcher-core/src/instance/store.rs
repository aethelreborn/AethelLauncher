use anyhow::Context as _;
use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: String,
    pub ram_mb: u64,
    pub java_path: Option<String>,
    pub renderer: String,
    pub auth_mode: String,
    pub created_at: String,
    pub last_launched: Option<String>,
}

pub struct InstanceStore {
    conn: Connection,
}

impl InstanceStore {
    pub fn open(db_path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create parent dir: {:?}", parent))?;
        }
        let conn = Connection::open(db_path)
            .with_context(|| format!("failed to open database: {:?}", db_path))?;
        Self::init(&conn)?;
        Ok(Self { conn })
    }

    fn init(conn: &Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS instances (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                mc_version TEXT NOT NULL,
                loader TEXT NOT NULL DEFAULT 'none',
                loader_version TEXT DEFAULT '',
                ram_mb INTEGER NOT NULL DEFAULT 2048,
                java_path TEXT,
                renderer TEXT NOT NULL DEFAULT 'auto',
                auth_mode TEXT NOT NULL DEFAULT 'offline',
                created_at TEXT NOT NULL,
                last_launched TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_instances_last_launched ON instances(last_launched DESC);"
        ).context("failed to initialize database schema")?;
        Ok(())
    }

    pub fn list(&self) -> anyhow::Result<Vec<Instance>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, mc_version, loader, loader_version, ram_mb, java_path, renderer, auth_mode, created_at, last_launched FROM instances ORDER BY last_launched DESC NULLS LAST"
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Instance {
                id: row.get(0)?,
                name: row.get(1)?,
                mc_version: row.get(2)?,
                loader: row.get(3)?,
                loader_version: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                ram_mb: row.get(5)?,
                java_path: row.get(6)?,
                renderer: row.get(7)?,
                auth_mode: row.get(8)?,
                created_at: row.get(9)?,
                last_launched: row.get(10)?,
            })
        })?;
        let instances: Vec<Instance> = rows.collect::<Result<Vec<_>, _>>()?;
        Ok(instances)
    }

    pub fn create(&mut self, instance: &Instance) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO instances (id, name, mc_version, loader, loader_version, ram_mb, java_path, renderer, auth_mode, created_at, last_launched)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![
                &instance.id, &instance.name, &instance.mc_version,
                &instance.loader, &instance.loader_version, instance.ram_mb,
                &instance.java_path, &instance.renderer, &instance.auth_mode,
                &instance.created_at,
                &instance.last_launched,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> anyhow::Result<usize> {
        let rows = self
            .conn
            .execute("DELETE FROM instances WHERE id = ?", rusqlite::params![id])?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_instance_store_crud() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let db_path = tmp.path().join("instances.db");
        let mut store = InstanceStore::open(&db_path).expect("open store");

        let instance = Instance {
            id: Uuid::new_v4().to_string(),
            name: "Test Instance".to_string(),
            mc_version: "1.21.11".to_string(),
            loader: "fabric".to_string(),
            loader_version: "0.16.0".to_string(),
            ram_mb: 4096,
            java_path: None,
            renderer: "auto".to_string(),
            auth_mode: "offline".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
            last_launched: None,
        };

        store.create(&instance).expect("create instance");
        let instances = store.list().expect("list instances");
        assert_eq!(instances.len(), 1);

        store.delete(&instance.id).expect("delete instance");
        let instances = store.list().expect("list after delete");
        assert_eq!(instances.len(), 0);
    }

    #[test]
    fn test_list_tolerates_null_loader_version() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let db_path = tmp.path().join("instances.db");
        let store = InstanceStore::open(&db_path).expect("open store");
        store
            .conn
            .execute(
                "INSERT INTO instances (id, name, mc_version, loader, loader_version, ram_mb, renderer, auth_mode, created_at)
                 VALUES ('legacy', 'Legacy', '1.20.1', 'none', NULL, 2048, 'auto', 'offline', '2026-01-01T00:00:00Z')",
                [],
            )
            .expect("insert legacy row");

        let instances = store.list().expect("list with null loader_version");
        assert_eq!(instances.len(), 1);
        assert_eq!(instances[0].loader_version, "");
    }
}
