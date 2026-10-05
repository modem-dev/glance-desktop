//! SQLite-backed execution and artifact records. Only the Code Mode service writes them.
use async_trait::async_trait;
use incurs_codemode::{ArtifactRef, ArtifactStore, ExecutionState, RuntimeStore, Snippet};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Mutex};

pub(super) struct Store(Mutex<Connection>);

impl Store {
    pub(super) fn open(path: &Path) -> Result<Self, String> {
        use std::os::unix::fs::PermissionsExt;
        let connection = Connection::open(path).map_err(|e| e.to_string())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        connection.execute_batch(
            "PRAGMA journal_mode=DELETE;
             CREATE TABLE IF NOT EXISTS executions (id TEXT PRIMARY KEY, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS snippets (id TEXT PRIMARY KEY, data TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS artifacts (id TEXT PRIMARY KEY, owner TEXT NOT NULL, data TEXT NOT NULL);"
        ).map_err(|e| e.to_string())?;
        Ok(Self(Mutex::new(connection)))
    }

    pub(super) fn recover_interrupted(&self) -> Result<(), String> {
        for mut execution in self.list::<ExecutionState>("executions")? {
            if matches!(execution.status, incurs_codemode::ExecutionStatus::Running) {
                execution.status = incurs_codemode::ExecutionStatus::Error;
                execution.error = Some("Code Mode service stopped before this execution finished. Completed edits were not replayed.".into());
                self.put("executions", &execution.id, &execution)?;
            }
        }
        Ok(())
    }

    fn get<T: DeserializeOwned>(&self, table: &str, id: &str) -> Result<Option<T>, String> {
        let connection = self.0.lock().map_err(|e| e.to_string())?;
        let value: Option<String> = connection
            .query_row(
                &format!("SELECT data FROM {table} WHERE id=?1"),
                [id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        value
            .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .transpose()
    }

    fn put<T: Serialize>(&self, table: &str, id: &str, value: &T) -> Result<(), String> {
        let data = serde_json::to_string(value).map_err(|e| e.to_string())?;
        self.0.lock().map_err(|e| e.to_string())?.execute(
            &format!("INSERT INTO {table}(id,data) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data"),
            params![id, data]
        ).map(|_| ()).map_err(|e| e.to_string())
    }

    fn list<T: DeserializeOwned>(&self, table: &str) -> Result<Vec<T>, String> {
        let connection = self.0.lock().map_err(|e| e.to_string())?;
        let mut query = connection
            .prepare(&format!("SELECT data FROM {table} ORDER BY id"))
            .map_err(|e| e.to_string())?;
        query
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .map(|row| {
                serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
            })
            .collect()
    }

    fn delete(&self, table: &str, id: &str) -> Result<bool, String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .execute(&format!("DELETE FROM {table} WHERE id=?1"), [id])
            .map(|n| n > 0)
            .map_err(|e| e.to_string())
    }
}

#[async_trait]
impl RuntimeStore for Store {
    async fn get_execution(&self, id: &str) -> Result<Option<ExecutionState>, String> {
        self.get("executions", id)
    }
    async fn put_execution(&self, execution: &ExecutionState) -> Result<(), String> {
        self.put("executions", &execution.id, execution)
    }
    async fn list_executions(&self) -> Result<Vec<ExecutionState>, String> {
        self.list("executions")
    }
    async fn delete_execution(&self, id: &str) -> Result<(), String> {
        let mut connection = self.0.lock().map_err(|e| e.to_string())?;
        let tx = connection.transaction().map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM executions WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM artifacts WHERE owner=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }
    async fn get_snippet(&self, name: &str) -> Result<Option<Snippet>, String> {
        self.get("snippets", name)
    }
    async fn put_snippet(&self, snippet: &Snippet) -> Result<(), String> {
        self.put("snippets", &snippet.name, snippet)
    }
    async fn list_snippets(&self) -> Result<Vec<Snippet>, String> {
        self.list("snippets")
    }
    async fn delete_snippet(&self, name: &str) -> Result<bool, String> {
        self.delete("snippets", name)
    }
}

#[async_trait]
impl ArtifactStore for Store {
    async fn put(&self, execution_id: &str, value: &Value) -> Result<ArtifactRef, String> {
        let data = serde_json::to_string(value).map_err(|e| e.to_string())?;
        let id = format!(
            "{:x}",
            Sha256::digest([execution_id.as_bytes(), data.as_bytes()].concat())
        );
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .execute(
                "INSERT OR IGNORE INTO artifacts(id,owner,data) VALUES(?1,?2,?3)",
                params![id, execution_id, data],
            )
            .map_err(|e| e.to_string())?;
        Ok(ArtifactRef {
            id,
            execution_id: execution_id.into(),
            bytes: data.len(),
            preview: data.chars().take(160).collect(),
        })
    }
    async fn get(&self, execution_id: &str, artifact_id: &str) -> Result<Option<Value>, String> {
        let connection = self.0.lock().map_err(|e| e.to_string())?;
        let value: Option<String> = connection
            .query_row(
                "SELECT data FROM artifacts WHERE id=?1 AND owner=?2",
                params![artifact_id, execution_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        value
            .map(|s| serde_json::from_str(&s).map_err(|e| e.to_string()))
            .transpose()
    }
    async fn delete_execution(&self, execution_id: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|e| e.to_string())?
            .execute("DELETE FROM artifacts WHERE owner=?1", [execution_id])
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
