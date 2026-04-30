//! Aether Provenance Store
//! 
//! SQLite-backed provenance tracking for Aether language artifacts.
//! Provides immutable storage and querying of provenance tags.

use aether_core::*;
use rusqlite::{Connection, params, Result as SqlResult};
use std::path::Path;
use std::str::FromStr;
use thiserror::Error;
use uuid::Uuid;

/// Errors from provenance store operations
#[derive(Error, Debug)]
pub enum ProvenanceError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Schema version mismatch: expected {expected}, found {found}")]
    SchemaVersionMismatch { expected: i64, found: i64 },
    #[error("Tag not found: {id}")]
    TagNotFound { id: Uuid },
    #[error("Invalid tag data: {0}")]
    InvalidTagData(String),
}

/// SQLite-backed provenance store with schema versioning
pub struct ProvenanceStore {
    conn: Connection,
}

impl ProvenanceStore {
    /// Current schema version
    const SCHEMA_VERSION: i64 = 1;
    
    /// Open or create a provenance store at the given path
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, ProvenanceError> {
        let mut conn = Connection::open(path)?;
        
        // Enable foreign keys
        conn.pragma_update(None, "foreign_keys", &1)?;
        
        // Initialize schema if needed
        Self::ensure_schema(&mut conn)?;
        
        Ok(ProvenanceStore { conn })
    }
    
    /// Ensure database schema is up to date
    fn ensure_schema(conn: &mut Connection) -> Result<(), ProvenanceError> {
        // Create schema_version table if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
            [],
        )?;
        
        // Check current schema version
        let current_version: Result<i64, _> = conn.query_row(
            "SELECT version FROM schema_version ORDER BY version DESC LIMIT 1",
            [],
            |row| row.get(0),
        );
        
        match current_version {
            Ok(version) => {
                if version != Self::SCHEMA_VERSION {
                    return Err(ProvenanceError::SchemaVersionMismatch {
                        expected: Self::SCHEMA_VERSION,
                        found: version,
                    });
                }
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // New database, initialize schema
                Self::initialize_schema(conn)?;
            }
            Err(e) => return Err(ProvenanceError::Database(e)),
        }
        
        Ok(())
    }
    
    /// Initialize the database schema
    fn initialize_schema(conn: &mut Connection) -> Result<(), ProvenanceError> {
        // Insert schema version
        conn.execute(
            "INSERT INTO schema_version (version) VALUES (?)",
            params![Self::SCHEMA_VERSION],
        )?;
        
        // Create provenance_tags table
        conn.execute(
            "CREATE TABLE provenance_tags (
                id TEXT PRIMARY KEY,                    -- UUID string
                author TEXT NOT NULL,
                model TEXT,
                timestamp TEXT NOT NULL,
                prompt TEXT,
                confidence REAL NOT NULL,
                parents TEXT,                           -- JSON array of UUID strings
                version TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                parent_id TEXT,                         -- For updates/chain
                FOREIGN KEY (parent_id) REFERENCES provenance_tags(id)
            )",
            [],
        )?;
        
        // Create indexes for common queries
        conn.execute("CREATE INDEX idx_provenance_tags_author ON provenance_tags(author)", [],)?;
        conn.execute("CREATE INDEX idx_provenance_tags_prompt ON provenance_tags(prompt)", [],)?;
        conn.execute("CREATE INDEX idx_provenance_tags_confidence ON provenance_tags(confidence)", [],)?;
        conn.execute("CREATE INDEX idx_provenance_tags_parent_id ON provenance_tags(parent_id)", [],)?;
        conn.execute("CREATE INDEX idx_provenance_tags_timestamp ON provenance_tags(timestamp)", [],)?;
        
        Ok(())
    }
    
    /// Insert a new provenance tag (immutable)
    pub fn insert(&mut self, tag: &ProvenanceTag) -> Result<Uuid, ProvenanceError> {
        let id = tag.id;
        let parents_json = serde_json::to_string(&tag.parents)
            .map_err(|e| ProvenanceError::InvalidTagData(e.to_string()))?;
        
        self.conn.execute(
            "INSERT INTO provenance_tags (
                id, author, model, timestamp, prompt, confidence, parents, version
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                id.to_string(),
                tag.author.to_string(),
                tag.model.as_ref().map(|m| m.to_string()),
                tag.timestamp.to_rfc3339(),
                tag.prompt.as_ref().map(|p| p.clone()),
                tag.confidence,
                parents_json,
                tag.version,
            ],
        )?;
        
        Ok(id)
    }
    
    /// Insert a tag as an update/chain of an existing tag
    pub fn insert_with_parent(&mut self, tag: &ProvenanceTag, parent_id: &Uuid) -> Result<Uuid, ProvenanceError> {
        let id = tag.id;
        let parents_json = serde_json::to_string(&tag.parents)
            .map_err(|e| ProvenanceError::InvalidTagData(e.to_string()))?;
        
        self.conn.execute(
            "INSERT INTO provenance_tags (
                id, author, model, timestamp, prompt, confidence, parents, version, parent_id
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                id.to_string(),
                tag.author.to_string(),
                tag.model.as_ref().map(|m| m.to_string()),
                tag.timestamp.to_rfc3339(),
                tag.prompt.as_ref().map(|p| p.clone()),
                tag.confidence,
                parents_json,
                tag.version,
                parent_id.to_string(),
            ],
        )?;
        
        Ok(id)
    }
    
    /// Query tags by prompt hash
    pub fn by_prompt(&self, prompt_hash: &str) -> Result<Vec<ProvenanceTag>, ProvenanceError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, model, timestamp, prompt, confidence, parents, version
             FROM provenance_tags 
             WHERE prompt = ? OR prompt LIKE ?
             ORDER BY timestamp DESC"
        )?;
        
        let tag_iter = stmt.query_map(
            params![prompt_hash, format!("%{}%", prompt_hash)],
            |row| self.row_to_tag(row),
        )?;
        
        let mut tags = Vec::new();
        for tag_result in tag_iter {
            tags.push(tag_result?);
        }
        
        Ok(tags)
    }
    
    /// Query tags by author
    pub fn by_author(&self, author: &AuthorType) -> Result<Vec<ProvenanceTag>, ProvenanceError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, model, timestamp, prompt, confidence, parents, version
             FROM provenance_tags 
             WHERE author = ?
             ORDER BY timestamp DESC"
        )?;
        
        let tag_iter = stmt.query_map(
            params![author.to_string()],
            |row| self.row_to_tag(row),
        )?;
        
        let mut tags = Vec::new();
        for tag_result in tag_iter {
            tags.push(tag_result?);
        }
        
        Ok(tags)
    }
    
    /// Query the chain of tags starting from a given node_id
    pub fn chain(&self, node_id: &Uuid) -> Result<Vec<ProvenanceTag>, ProvenanceError> {
        let mut tags = Vec::new();
        let mut current_id = Some(*node_id);
        
        while let Some(id) = current_id {
            let tag = self.by_id(&id)?;
            current_id = tag.parents.first().copied();
            tags.push(tag);
        }
        
        Ok(tags)
    }
    
    /// Query tags with confidence below threshold
    pub fn confidence_below(&self, threshold: f64) -> Result<Vec<ProvenanceTag>, ProvenanceError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, model, timestamp, prompt, confidence, parents, version
             FROM provenance_tags 
             WHERE confidence < ?
             ORDER BY confidence ASC"
        )?;
        
        let tag_iter = stmt.query_map(
            params![threshold],
            |row| self.row_to_tag(row),
        )?;
        
        let mut tags = Vec::new();
        for tag_result in tag_iter {
            tags.push(tag_result?);
        }
        
        Ok(tags)
    }
    
    /// Get a tag by its ID
    pub fn by_id(&self, id: &Uuid) -> Result<ProvenanceTag, ProvenanceError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, model, timestamp, prompt, confidence, parents, version
             FROM provenance_tags 
             WHERE id = ?"
        )?;
        
        let tag_result = stmt.query_row(
            params![id.to_string()],
            |row| self.row_to_tag(row),
        )?;
        
        Ok(tag_result)
    }
    
    /// Get all tags
    pub fn all(&self) -> Result<Vec<ProvenanceTag>, ProvenanceError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, author, model, timestamp, prompt, confidence, parents, version
             FROM provenance_tags 
             ORDER BY timestamp DESC"
        )?;
        
        let tag_iter = stmt.query_map([], |row| self.row_to_tag(row))?;
        
        let mut tags = Vec::new();
        for tag_result in tag_iter {
            tags.push(tag_result?);
        }
        
        Ok(tags)
    }
    
    /// Convert a database row to a ProvenanceTag
    fn row_to_tag(&self, row: &rusqlite::Row) -> SqlResult<ProvenanceTag> {
        let id_str: String = row.get(0)?;
        let id = Uuid::parse_str(&id_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(0, "uuid".to_string(), rusqlite::types::Type::Text))?;
        
        let author_str: String = row.get(1)?;
        let author = AuthorType::from_str(&author_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(1, "author".to_string(), rusqlite::types::Type::Text))?;
        
        let model: Option<String> = row.get(2)?;
        
        let timestamp_str: String = row.get(3)?;
        let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(3, "timestamp".to_string(), rusqlite::types::Type::Text))?
            .with_timezone(&chrono::Utc);
        
        let prompt: Option<String> = row.get(4)?;
        
        let confidence: f64 = row.get(5)?;
        
        let parents_str: String = row.get(6)?;
        let parents: Vec<Uuid> = serde_json::from_str(&parents_str)
            .map_err(|_| rusqlite::Error::InvalidColumnType(6, "parents".to_string(), rusqlite::types::Type::Text))?;
        
        let version: String = row.get(7)?;
        
        Ok(ProvenanceTag {
            id,
            author,
            model,
            timestamp,
            prompt,
            confidence,
            parents,
            version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use tempfile::tempdir;

    #[test]
    fn test_schema_initialization() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        
        let store = ProvenanceStore::open(&db_path).unwrap();
        
        // Check that schema version is set
        let version: i64 = store.conn.query_row(
            "SELECT version FROM schema_version",
            [],
            |row| row.get(0),
        ).unwrap();
        
        assert_eq!(version, ProvenanceStore::SCHEMA_VERSION);
    }

    #[test]
    fn test_insert_and_query() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        
        let mut store = ProvenanceStore::open(&db_path).unwrap();
        
        let tag = ProvenanceTag {
            id: Uuid::new_v4(),
            author: AuthorType::Human,
            model: None,
            timestamp: chrono::Utc::now(),
            prompt: Some("test prompt".to_string()),
            confidence: 0.95,
            parents: vec![],
            version: "1.0".to_string(),
        };
        
        let inserted_id = store.insert(&tag).unwrap();
        assert_eq!(inserted_id, tag.id);
        
        let retrieved_tag = store.by_id(&tag.id).unwrap();
        assert_eq!(retrieved_tag.id, tag.id);
        assert_eq!(retrieved_tag.author, tag.author);
        assert_eq!(retrieved_tag.confidence, tag.confidence);
    }

    #[test]
    fn test_by_author() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        
        let mut store = ProvenanceStore::open(&db_path).unwrap();
        
        let tag1 = ProvenanceTag {
            id: Uuid::new_v4(),
            author: AuthorType::Human,
            model: None,
            timestamp: chrono::Utc::now(),
            prompt: Some("test prompt 1".to_string()),
            confidence: 0.95,
            parents: vec![],
            version: "1.0".to_string(),
        };
        
        let tag2 = ProvenanceTag {
            id: Uuid::new_v4(),
            author: AuthorType::AI("claude-sonnet-4-6".to_string()),
            model: Some("claude-sonnet-4-6".to_string()),
            timestamp: chrono::Utc::now(),
            prompt: Some("test prompt 2".to_string()),
            confidence: 0.85,
            parents: vec![],
            version: "1.0".to_string(),
        };
        
        store.insert(&tag1).unwrap();
        store.insert(&tag2).unwrap();
        
        let human_tags = store.by_author(&AuthorType::Human).unwrap();
        assert_eq!(human_tags.len(), 1);
        assert_eq!(human_tags[0].id, tag1.id);
        
        let ai_tags = store.by_author(&AuthorType::AI("claude-sonnet-4-6".to_string())).unwrap();
        assert_eq!(ai_tags.len(), 1);
        assert_eq!(ai_tags[0].id, tag2.id);
    }

    #[test]
    fn test_confidence_below() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        
        let mut store = ProvenanceStore::open(&db_path).unwrap();
        
        let tag1 = ProvenanceTag {
            id: Uuid::new_v4(),
            author: AuthorType::Human,
            model: None,
            timestamp: chrono::Utc::now(),
            prompt: Some("test prompt 1".to_string()),
            confidence: 0.95,
            parents: vec![],
            version: "1.0".to_string(),
        };
        
        let tag2 = ProvenanceTag {
            id: Uuid::new_v4(),
            author: AuthorType::Human,
            model: None,
            timestamp: chrono::Utc::now(),
            prompt: Some("test prompt 2".to_string()),
            confidence: 0.75,
            parents: vec![],
            version: "1.0".to_string(),
        };
        
        store.insert(&tag1).unwrap();
        store.insert(&tag2).unwrap();
        
        let low_confidence = store.confidence_below(0.8).unwrap();
        assert_eq!(low_confidence.len(), 1);
        assert_eq!(low_confidence[0].id, tag2.id);
    }
}
