//! Session management for Aether wellbeing system.
//! Reads/writes .aether-session in project root.
//! Reads .aether-wellbeing for config (falls back to defaults).

use aether_core::SessionState;
use std::path::Path;

const SESSION_FILE: &str = ".aether-session";

pub struct SessionManager {
    pub state: SessionState,
}

impl SessionManager {
    /// Load or create session for given workspace root
    pub fn load(root: &Path) -> Self {
        let state = load_state(root);
        Self { state }
    }

    /// Check if cooldown is active — call before any aether command
    pub fn check_cooldown(&self) -> Option<CooldownBlock> {
        if self.state.cooldown_active() {
            Some(CooldownBlock {
                minutes_remaining: self.state.cooldown_remaining_minutes(),
            })
        } else {
            None
        }
    }
}

pub struct CooldownBlock {
    pub minutes_remaining: u64,
}


fn load_state(root: &Path) -> SessionState {
    let path = root.join(SESSION_FILE);
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(state) = serde_json::from_str(&content) {
            return state;
        }
    }
    // No existing session — start fresh
    let state = SessionState::new();
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        let _ = std::fs::write(path, json);
    }
    state
}
