use rusqlite::params;
use crate::store::{ProvStore, StoreResult};

/// Summary of a node's provenance chain
#[derive(Debug)]
pub struct ProvChain {
    pub entries: Vec<ProvSummary>,
}

#[derive(Debug)]
pub struct ProvSummary {
    pub id: u64,
    pub source: String,
    pub author: String,
    pub confidence: f64,
    pub timestamp: String,
}

impl ProvStore {
    /// Walk the parent chain for a given prov_id.
    /// Returns from oldest ancestor to the given id.
    pub fn chain(&self, id: u64) -> StoreResult<ProvChain> {
        // Recursive CTE — walks parent_id chain
        let mut stmt = self.conn.prepare(r#"
            WITH RECURSIVE chain(id, source, author, confidence, timestamp, parent_id) AS (
                SELECT id, source, author, confidence, timestamp, parent_id
                FROM prov_entries WHERE id = ?1
                UNION ALL
                SELECT p.id, p.source, p.author, p.confidence, p.timestamp, p.parent_id
                FROM prov_entries p
                INNER JOIN chain c ON p.id = c.parent_id
            )
            SELECT id, source, author, confidence, timestamp FROM chain ORDER BY id ASC
        "#)?;

        let entries = stmt.query_map(params![id as i64], |row| {
            Ok(ProvSummary {
                id: row.get::<_, i64>(0)? as u64,
                source: row.get(1)?,
                author: row.get(2)?,
                confidence: row.get(3)?,
                timestamp: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(ProvChain { entries })
    }

    /// All entries for a given session, ordered by id
    pub fn session_entries(&self, session_id: &str) -> StoreResult<Vec<ProvSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source, author, confidence, timestamp
             FROM prov_entries WHERE session_id = ?1 ORDER BY id ASC"
        )?;

        let entries = stmt.query_map(params![session_id], |row| {
            Ok(ProvSummary {
                id: row.get::<_, i64>(0)? as u64,
                source: row.get(1)?,
                author: row.get(2)?,
                confidence: row.get(3)?,
                timestamp: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

        Ok(entries)
    }
}
