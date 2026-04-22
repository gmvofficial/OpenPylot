//! Session persistence with SQLite + FTS5 search.
//! Inspired by Hermes' SQLite session store with full-text search
//! across 100k+ messages, auto-generated titles, and session chains.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A persisted conversation session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub source: String, // "cli", "api", "telegram", etc.
    pub parent_session_id: Option<String>, // For session chains (compression splits)
    pub metadata: serde_json::Value,
}

/// A single message within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub tool_calls_json: Option<String>,
    pub thinking: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Session store backed by SQLite with FTS5 full-text search.
pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    /// Open or create the session database.
    pub fn open(data_dir: &Path) -> Result<Self> {
        let db_path = data_dir.join("sessions.db");
        let conn = Connection::open(&db_path)
            .context("Failed to open sessions database")?;

        let store = Self { conn };
        store.init_schema()?;
        Ok(store)
    }

    /// Initialize database schema.
    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT 'Untitled',
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now')),
                message_count INTEGER NOT NULL DEFAULT 0,
                source TEXT NOT NULL DEFAULT 'cli',
                parent_session_id TEXT,
                metadata TEXT NOT NULL DEFAULT '{}'
            );

            CREATE TABLE IF NOT EXISTS session_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                role TEXT NOT NULL,
                content TEXT NOT NULL DEFAULT '',
                tool_call_id TEXT,
                tool_calls_json TEXT,
                thinking TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_messages_session ON session_messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_messages_role ON session_messages(role);
            CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at DESC);

            -- FTS5 virtual table for full-text search across messages
            CREATE VIRTUAL TABLE IF NOT EXISTS session_messages_fts USING fts5(
                content,
                content=session_messages,
                content_rowid=id
            );

            -- Triggers to keep FTS in sync
            CREATE TRIGGER IF NOT EXISTS messages_ai AFTER INSERT ON session_messages BEGIN
                INSERT INTO session_messages_fts(rowid, content) VALUES (new.id, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS messages_ad AFTER DELETE ON session_messages BEGIN
                INSERT INTO session_messages_fts(session_messages_fts, rowid, content)
                    VALUES('delete', old.id, old.content);
            END;

            CREATE TRIGGER IF NOT EXISTS messages_au AFTER UPDATE ON session_messages BEGIN
                INSERT INTO session_messages_fts(session_messages_fts, rowid, content)
                    VALUES('delete', old.id, old.content);
                INSERT INTO session_messages_fts(rowid, content) VALUES (new.id, new.content);
            END;
            "
        ).context("Failed to create sessions schema")?;

        Ok(())
    }

    /// Create a new session.
    pub fn create_session(&self, id: &str, source: &str) -> Result<Session> {
        let now = Utc::now();
        self.conn.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at, source) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, "Untitled", now.to_rfc3339(), now.to_rfc3339(), source],
        )?;

        Ok(Session {
            id: id.to_string(),
            title: "Untitled".to_string(),
            created_at: now,
            updated_at: now,
            message_count: 0,
            source: source.to_string(),
            parent_session_id: None,
            metadata: serde_json::json!({}),
        })
    }

    /// Update session title.
    pub fn update_title(&self, session_id: &str, title: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, Utc::now().to_rfc3339(), session_id],
        )?;
        Ok(())
    }

    /// Add a message to a session.
    pub fn add_message(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        tool_call_id: Option<&str>,
        tool_calls_json: Option<&str>,
        thinking: Option<&str>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO session_messages (session_id, role, content, tool_call_id, tool_calls_json, thinking)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![session_id, role, content, tool_call_id, tool_calls_json, thinking],
        )?;

        let msg_id = self.conn.last_insert_rowid();

        // Update session stats
        self.conn.execute(
            "UPDATE sessions SET message_count = message_count + 1, updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id],
        )?;

        Ok(msg_id)
    }

    /// List recent sessions.
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<Session>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, created_at, updated_at, message_count, source, parent_session_id, metadata
             FROM sessions ORDER BY updated_at DESC LIMIT ?1"
        )?;

        let sessions = stmt.query_map(params![limit], |row| {
            Ok(Session {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get::<_, String>(2)?
                    .parse()
                    .unwrap_or_else(|_| Utc::now()),
                updated_at: row.get::<_, String>(3)?
                    .parse()
                    .unwrap_or_else(|_| Utc::now()),
                message_count: row.get::<_, i64>(4)? as usize,
                source: row.get(5)?,
                parent_session_id: row.get(6)?,
                metadata: row.get::<_, String>(7)?
                    .parse()
                    .unwrap_or(serde_json::json!({})),
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    /// Get messages for a session.
    pub fn get_messages(&self, session_id: &str, limit: Option<usize>) -> Result<Vec<SessionMessage>> {
        let query = if let Some(lim) = limit {
            format!(
                "SELECT id, session_id, role, content, tool_call_id, tool_calls_json, thinking, created_at
                 FROM session_messages WHERE session_id = ?1 ORDER BY id DESC LIMIT {}",
                lim
            )
        } else {
            "SELECT id, session_id, role, content, tool_call_id, tool_calls_json, thinking, created_at
             FROM session_messages WHERE session_id = ?1 ORDER BY id ASC".to_string()
        };

        let mut stmt = self.conn.prepare(&query)?;
        let messages = stmt.query_map(params![session_id], |row| {
            Ok(SessionMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_call_id: row.get(4)?,
                tool_calls_json: row.get(5)?,
                thinking: row.get(6)?,
                created_at: row.get::<_, String>(7)?
                    .parse()
                    .unwrap_or_else(|_| Utc::now()),
            })
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(messages)
    }

    /// Full-text search across all session messages.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(SessionMessage, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT m.id, m.session_id, m.role, m.content, m.tool_call_id, m.tool_calls_json, m.thinking, m.created_at,
                    rank
             FROM session_messages_fts fts
             JOIN session_messages m ON m.id = fts.rowid
             WHERE session_messages_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2"
        )?;

        let results = stmt.query_map(params![query, limit], |row| {
            let msg = SessionMessage {
                id: row.get(0)?,
                session_id: row.get(1)?,
                role: row.get(2)?,
                content: row.get(3)?,
                tool_call_id: row.get(4)?,
                tool_calls_json: row.get(5)?,
                thinking: row.get(6)?,
                created_at: row.get::<_, String>(7)?
                    .parse()
                    .unwrap_or_else(|_| Utc::now()),
            };
            let rank: f64 = row.get(8)?;
            Ok((msg, rank))
        })?.collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Delete a session and all its messages.
    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        // Delete messages first (FTS triggers will fire)
        self.conn.execute(
            "DELETE FROM session_messages WHERE session_id = ?1",
            params![session_id],
        )?;

        let deleted = self.conn.execute(
            "DELETE FROM sessions WHERE id = ?1",
            params![session_id],
        )?;

        Ok(deleted > 0)
    }

    /// Get total stats.
    pub fn stats(&self) -> Result<(usize, usize)> {
        let session_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM sessions", [], |row| row.get(0),
        )?;
        let message_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM session_messages", [], |row| row.get(0),
        )?;
        Ok((session_count as usize, message_count as usize))
    }
}
