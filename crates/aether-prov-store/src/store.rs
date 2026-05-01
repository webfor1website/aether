use rusqlite::{Connection, params, OptionalExtension};
use thiserror::Error;
use crate::schema::SCHEMA;
use aether_ir::expr::ProvId;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub type StoreResult<T> = Result<T, StoreError>;

pub struct ProvStore {
    pub conn: Connection,
    pub session_id: String,
}

impl ProvStore {
    pub fn open(path: &str, session_id: String) -> StoreResult<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn, session_id })
    }

    /// Insert a raw provenance record. Returns assigned ProvId.
    /// Takes plain strings to avoid ToSql trait issues with custom types.
    pub fn insert_raw(
        &self,
        function_name: &str,
        author: &str,
        prompt: Option<&str>,
        confidence: f64,
        timestamp: &str,
        parents_json: &str,   // serialize Vec<Uuid> as JSON before calling
        model: Option<&str>,
        file_path: Option<&str>,
    ) -> StoreResult<ProvId> {
        self.conn.execute(
            r#"INSERT INTO prov_entries
               (function_name, author, prompt, confidence, timestamp, parents_json, model, session_id, file_path)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"#,
            params![
                function_name,
                author,
                prompt,
                confidence,
                timestamp,
                parents_json,
                model,
                self.session_id,
                file_path,
            ],
        )?;
        Ok(self.conn.last_insert_rowid() as ProvId)
    }

    pub fn session_trust_score(&self) -> StoreResult<f64> {
        let score: Option<f64> = self.conn.query_row(
            "SELECT AVG(COALESCE(adjusted_confidence, confidence)) FROM prov_entries WHERE session_id = ?1",
            params![self.session_id],
            |row| row.get(0),
        )?;
        Ok(score.unwrap_or(0.0))
    }

    pub fn begin_session(&self, file_path: &str) -> StoreResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO exec_sessions (session_id, file_path, started_at)
             VALUES (?1, ?2, datetime('now'))",
            params![self.session_id, file_path],
        )?;
        Ok(())
    }

    pub fn end_session(&self) -> StoreResult<f64> {
        let score = self.session_trust_score()?;
        self.conn.execute(
            "UPDATE exec_sessions SET ended_at = datetime('now'), trust_score = ?1
             WHERE session_id = ?2",
            params![score, self.session_id],
        )?;
        Ok(score)
    }

    pub fn get_function_records(&self) -> StoreResult<Vec<FunctionRecord>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT function_name, author, prompt, confidence, timestamp, model, file_path
               FROM prov_entries 
               WHERE session_id = ?1
               ORDER BY confidence ASC"#,
        )?;
        
        let records = stmt.query_map(params![self.session_id], |row| {
            Ok(FunctionRecord {
                function_name: row.get(0)?,
                author: row.get(1)?,
                prompt: row.get(2)?,
                confidence: row.get(3)?,
                timestamp: row.get(4)?,
                model: row.get(5)?,
                file_path: row.get(6)?,
            })
        })?;
        
        let mut result = Vec::new();
        for record in records {
            result.push(record?);
        }
        Ok(result)
    }

    pub fn get_replay_records(&self) -> StoreResult<Vec<FunctionRecord>> {
        let mut stmt = self.conn.prepare(
            r#"SELECT function_name, author, prompt, confidence, timestamp, model, file_path
               FROM prov_entries 
               WHERE session_id = ?1
               ORDER BY timestamp ASC"#,
        )?;
        
        let records = stmt.query_map(params![self.session_id], |row| {
            Ok(FunctionRecord {
                function_name: row.get(0)?,
                author: row.get(1)?,
                prompt: row.get(2)?,
                confidence: row.get(3)?,
                timestamp: row.get(4)?,
                model: row.get(5)?,
                file_path: row.get(6)?,
            })
        })?;
        
        let mut result = Vec::new();
        for record in records {
            result.push(record?);
        }
        Ok(result)
    }

    /// Record a function call with call depth for weighted trust scoring
    pub fn record_function_call(&self, function_name: &str, call_depth: usize) -> StoreResult<()> {
        // First, find the most recent entry for this function in the current session
        let id: Option<i64> = self.conn.query_row(
            r#"SELECT id FROM prov_entries 
               WHERE session_id = ?1 AND function_name = ?2
               ORDER BY id DESC 
               LIMIT 1"#,
            params![self.session_id, function_name],
            |row| row.get(0),
        ).optional()?;
        
        // If we found an entry, update its call depth
        if let Some(target_id) = id {
            self.conn.execute(
                r#"UPDATE prov_entries 
                   SET call_depth = ?1 
                   WHERE id = ?2"#,
                params![call_depth as i64, target_id],
            )?;
        }
        
        Ok(())
    }

    /// Calculate weighted trust score based on call depth
    pub fn weighted_trust_score(&self) -> StoreResult<f64> {
        let score: Option<f64> = self.conn.query_row(
            r#"SELECT SUM(CASE 
                   WHEN adjusted_confidence IS NOT NULL 
                   THEN adjusted_confidence * (call_depth + 1)
                   ELSE confidence * (call_depth + 1)
               END) / SUM(call_depth + 1) 
               FROM prov_entries 
               WHERE session_id = ?1 AND function_name != 'main'"#,
            params![self.session_id],
            |row| row.get(0),
        )?;
        Ok(score.unwrap_or(0.0))
    }

    /// Calculate flat trust score (original simple average)
    pub fn flat_trust_score(&self) -> StoreResult<f64> {
        let score: Option<f64> = self.conn.query_row(
            r#"SELECT AVG(COALESCE(adjusted_confidence, confidence)) 
               FROM prov_entries 
               WHERE session_id = ?1 AND function_name != 'main'"#,
            params![self.session_id],
            |row| row.get(0),
        )?;
        Ok(score.unwrap_or(0.0))
    }

    /// Record an override for a blocked function
    pub fn record_override(&self, function_name: &str, file_path: &str) -> StoreResult<()> {
        self.conn.execute(
            r#"UPDATE prov_entries 
               SET event_type = 'override'
               WHERE session_id = ?1 AND function_name = ?2 AND file_path = ?3"#,
            params![self.session_id, function_name, file_path],
        )?;
        Ok(())
    }

    /// Record a quarantine for a blocked function
    pub fn record_quarantine(&self, function_name: &str, file_path: &str) -> StoreResult<()> {
        self.conn.execute(
            r#"UPDATE prov_entries 
               SET event_type = 'quarantine'
               WHERE session_id = ?1 AND function_name = ?2 AND file_path = ?3"#,
            params![self.session_id, function_name, file_path],
        )?;
        Ok(())
    }

    /// Check if a function is quarantined
    pub fn is_quarantined(&self, function_name: &str, file_path: &str) -> StoreResult<bool> {
        let count: i64 = self.conn.query_row(
            r#"SELECT COUNT(*) FROM prov_entries 
               WHERE session_id = ?1 AND function_name = ?2 AND file_path = ?3 AND event_type = 'quarantine'"#,
            params![self.session_id, function_name, file_path],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Evolve trust scores for all records in the current session (except quarantined)
    pub fn evolve_trust(&self, delta: f64) -> StoreResult<()> {
        self.conn.execute(
            r#"UPDATE prov_entries 
               SET adjusted_confidence = CASE 
                   WHEN adjusted_confidence IS NULL THEN 
                       MIN(1.0, MAX(0.0, confidence + ?1))
                   ELSE 
                       MIN(1.0, MAX(0.0, adjusted_confidence + ?1))
               END
               WHERE session_id = ?2 AND event_type != 'quarantine'"#,
            params![delta, self.session_id],
        )?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct FunctionRecord {
    pub function_name: String,
    pub author: String,
    pub prompt: Option<String>,
    pub confidence: f64,
    pub timestamp: String,
    pub model: Option<String>,
    pub file_path: Option<String>,
}
