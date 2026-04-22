use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

use super::types::*;

/// SQLite-backed persistence for sub-agent state.
pub struct SubAgentStore {
    conn: Mutex<Connection>,
}

impl SubAgentStore {
    pub fn open(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join("sub_agents.db");
        let conn = Connection::open(db_path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_schema()?;
        Ok(store)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sub_agents (
                id              TEXT PRIMARY KEY,
                name            TEXT NOT NULL,
                agent_type      TEXT NOT NULL DEFAULT 'Task',
                system_prompt   TEXT NOT NULL DEFAULT '',
                status          TEXT NOT NULL DEFAULT 'Pending',
                task            TEXT NOT NULL DEFAULT '',
                result          TEXT,
                error           TEXT,
                conversation_id TEXT,
                started_at      TEXT,
                completed_at    TEXT,
                created_at      TEXT NOT NULL DEFAULT (datetime('now')),
                timeout_secs    INTEGER NOT NULL DEFAULT 300,
                max_iterations  INTEGER NOT NULL DEFAULT 10
            );

            CREATE INDEX IF NOT EXISTS idx_sub_agents_status ON sub_agents(status);
            CREATE INDEX IF NOT EXISTS idx_sub_agents_created ON sub_agents(created_at DESC);",
        )?;
        Ok(())
    }

    /// Insert a new sub-agent record.
    pub fn insert(&self, config: &SubAgentConfig, task: &str, conversation_id: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sub_agents (id, name, agent_type, system_prompt, status, task, conversation_id, timeout_secs, max_iterations)
             VALUES (?1, ?2, ?3, ?4, 'Pending', ?5, ?6, ?7, ?8)",
            params![
                config.id,
                config.name,
                format!("{:?}", config.agent_type),
                config.system_prompt,
                task,
                conversation_id,
                config.timeout_secs,
                config.max_iterations,
            ],
        )?;
        Ok(())
    }

    /// Update status and timestamps.
    pub fn update_status(&self, id: &str, status: &str, started_at: Option<&str>, completed_at: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sub_agents SET status = ?1, started_at = COALESCE(?2, started_at), completed_at = COALESCE(?3, completed_at) WHERE id = ?4",
            params![status, started_at, completed_at, id],
        )?;
        Ok(())
    }

    /// Store the result of a completed sub-agent.
    pub fn set_result(&self, id: &str, result: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sub_agents SET result = ?1, status = 'Completed', completed_at = datetime('now') WHERE id = ?2",
            params![result, id],
        )?;
        Ok(())
    }

    /// Store the error of a failed sub-agent.
    pub fn set_error(&self, id: &str, error: &str, status: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sub_agents SET error = ?1, status = ?2, completed_at = datetime('now') WHERE id = ?3",
            params![error, status, id],
        )?;
        Ok(())
    }

    /// List all sub-agents, newest first.
    pub fn list(&self, limit: usize) -> Result<Vec<StoredSubAgent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, agent_type, status, task, result, error, conversation_id, started_at, completed_at, created_at
             FROM sub_agents ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(StoredSubAgent {
                id: row.get(0)?,
                name: row.get(1)?,
                agent_type: row.get(2)?,
                status: row.get(3)?,
                task: row.get(4)?,
                result: row.get(5)?,
                error: row.get(6)?,
                conversation_id: row.get(7)?,
                started_at: row.get(8)?,
                completed_at: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get a single sub-agent by ID.
    pub fn get(&self, id: &str) -> Result<Option<StoredSubAgent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, agent_type, status, task, result, error, conversation_id, started_at, completed_at, created_at
             FROM sub_agents WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(StoredSubAgent {
                id: row.get(0)?,
                name: row.get(1)?,
                agent_type: row.get(2)?,
                status: row.get(3)?,
                task: row.get(4)?,
                result: row.get(5)?,
                error: row.get(6)?,
                conversation_id: row.get(7)?,
                started_at: row.get(8)?,
                completed_at: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;
        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            _ => Ok(None),
        }
    }
}

/// A sub-agent record from the database.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StoredSubAgent {
    pub id: String,
    pub name: String,
    pub agent_type: String,
    pub status: String,
    pub task: String,
    pub result: Option<String>,
    pub error: Option<String>,
    pub conversation_id: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
}
