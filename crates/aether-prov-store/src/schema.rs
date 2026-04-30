pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS prov_entries (
    id           INTEGER PRIMARY KEY,
    function_name TEXT    NOT NULL DEFAULT '',
    author       TEXT    NOT NULL DEFAULT '',
    model        TEXT,
    prompt       TEXT,
    confidence   REAL    NOT NULL DEFAULT 1.0,
    timestamp    TEXT    NOT NULL DEFAULT '',
    parents_json TEXT    NOT NULL DEFAULT '[]',
    session_id   TEXT,
    file_path    TEXT,
    adjusted_confidence REAL,
    call_depth   INTEGER DEFAULT 0,
    event_type   TEXT    DEFAULT 'normal'  -- 'normal', 'override', 'quarantine'
);

CREATE TABLE IF NOT EXISTS exec_sessions (
    session_id  TEXT PRIMARY KEY,
    file_path   TEXT NOT NULL,
    started_at  TEXT NOT NULL,
    ended_at    TEXT,
    trust_score REAL
);

CREATE INDEX IF NOT EXISTS idx_prov_session ON prov_entries(session_id);
"#;
