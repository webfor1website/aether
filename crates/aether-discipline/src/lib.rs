//! Aether Discipline - Enforces read-verify-edit discipline for AI assistants
//! 
//! This crate provides the discipline layer that enforces the read-verify-edit
//! pattern to prevent AI assistants from making destructive changes without
//! proper verification.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisciplineConfig {
    pub workspace_root: PathBuf,
    pub session_id: String,
    pub enabled: bool,
}

impl DisciplineConfig {
    pub fn new(workspace_root: PathBuf, session_id: String) -> Self {
        Self {
            workspace_root,
            session_id,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditProvenance {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub target_file: PathBuf,
    pub action: String,
    pub verification_status: VerificationStatus,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationStatus {
    Pending,
    Verified,
    Failed(String),
}

#[derive(Debug, Error)]
pub enum DisciplineError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type DisciplineResult<T> = Result<T, DisciplineError>;

/// Main discipline engine that enforces read-verify-edit discipline
pub struct DisciplineEngine {
    config: DisciplineConfig,
    edit_history: Vec<EditProvenance>,
    workspace_cache: HashMap<PathBuf, String>,
}

impl DisciplineEngine {
    /// Create a new discipline engine with the given workspace root
    pub fn new(workspace_root: &Path) -> Self {
        let config = DisciplineConfig::new(
            workspace_root.to_path_buf(),
            Uuid::new_v4().to_string(),
        );
        
        Self {
            config,
            edit_history: Vec::new(),
            workspace_cache: HashMap::new(),
        }
    }
    
    /// Enforce discipline before making edits to a target file
    pub fn enforce_before_edit(&mut self, target_file: &Path, operation_type: &str) -> DisciplineResult<()> {
        if !self.config.enabled {
            return Ok(());
        }
        
        // Read the current file content and cache it
        let current_content = std::fs::read_to_string(target_file)?;
        self.workspace_cache.insert(target_file.to_path_buf(), current_content.clone());
        
        // Create provenance record with operation type
        let provenance = EditProvenance {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            target_file: target_file.to_path_buf(),
            action: format!("before_edit_{}", operation_type),
            verification_status: VerificationStatus::Verified,
            session_id: self.config.session_id.clone(),
        };
        
        self.edit_history.push(provenance);
        
        Ok(())
    }
    
    /// Enforce discipline before making edits to a target file (backward compatibility)
    pub fn enforce_before_edit_legacy(&mut self, target_file: &Path) -> DisciplineResult<()> {
        self.enforce_before_edit(target_file, "general")
    }
    
    /// Log provenance for successful operations
    pub fn log_provenance(&mut self, target_file: &Path, action: &str) -> DisciplineResult<()> {
        let provenance = EditProvenance {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            target_file: target_file.to_path_buf(),
            action: action.to_string(),
            verification_status: VerificationStatus::Verified,
            session_id: self.config.session_id.clone(),
        };
        
        self.edit_history.push(provenance);
        Ok(())
    }
    
    /// Require that a file was explicitly read this session
    pub fn require_read(&self, target_file: &Path) -> DisciplineResult<()> {
        if !self.config.enabled {
            return Ok(());
        }
        
        // Check if the file was cached (i.e., read) this session
        if !self.workspace_cache.contains_key(target_file) {
            return Err(DisciplineError::VerificationFailed(
                format!("File {:?} was not explicitly read this session", target_file)
            ));
        }
        
        Ok(())
    }
    
    /// Get the current edit history
    pub fn edit_history(&self) -> &[EditProvenance] {
        &self.edit_history
    }
    
    /// Get the workspace root
    pub fn workspace_root(&self) -> &Path {
        &self.config.workspace_root
    }
    
    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.config.session_id
    }
    
    /// Enable or disable discipline enforcement
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }
    
    /// Check if discipline is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[test]
    fn test_discipline_engine_creation() {
        let temp_dir = TempDir::new().unwrap();
        let engine = DisciplineEngine::new(temp_dir.path());
        
        assert!(engine.is_enabled());
        assert_eq!(engine.edit_history().len(), 0);
    }
    
    #[test]
    fn test_enforce_before_edit() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.aeth");
        fs::write(&test_file, "fn main() -> Unit {}").unwrap();
        
        let mut engine = DisciplineEngine::new(temp_dir.path());
        let result = engine.enforce_before_edit(&test_file, "test");
        
        assert!(result.is_ok());
        assert_eq!(engine.edit_history().len(), 1);
    }
    
    #[test]
    fn test_log_provenance() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.aeth");
        fs::write(&test_file, "fn main() -> Unit {}").unwrap();
        
        let mut engine = DisciplineEngine::new(temp_dir.path());
        engine.log_provenance(&test_file, "test_action").unwrap();
        
        assert_eq!(engine.edit_history().len(), 1);
        let provenance = &engine.edit_history()[0];
        assert_eq!(provenance.action, "test_action");
    }
}
