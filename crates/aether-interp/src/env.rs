use std::collections::HashMap;
use crate::value::Value;
use crate::error::InterpError;

/// Lexical environment — a stack of scopes.
/// Aether has no closures in v0.1, so this is a simple frame stack.
pub struct Env {
    frames: Vec<HashMap<String, Value>>,
}

impl Env {
    pub fn new() -> Self {
        Self { frames: vec![HashMap::new()] }
    }

    /// Push a new scope (function call or block)
    pub fn push_scope(&mut self) {
        self.frames.push(HashMap::new());
    }

    /// Pop the innermost scope
    pub fn pop_scope(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Define a binding in current scope
    pub fn define(&mut self, name: &str, value: Value) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_string(), value);
        }
    }

    /// Shadow a binding — Aether requires `shadow` keyword; checker enforces it.
    /// Interpreter just overwrites.
    pub fn shadow(&mut self, name: &str, value: Value) {
        // Walk frames from innermost out; shadow creates a new binding in current frame
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_string(), value);
        }
    }

    /// Resolve a name. Walks from innermost scope outward.
    pub fn get(&self, name: &str) -> Result<&Value, InterpError> {
        for frame in self.frames.iter().rev() {
            if let Some(v) = frame.get(name) {
                return Ok(v);
            }
        }
        Err(InterpError::UndefinedVar(name.to_string()))
    }

    /// Check if name exists (for shadow validation)
    pub fn exists(&self, name: &str) -> bool {
        self.frames.iter().any(|f| f.contains_key(name))
    }

    /// Save current environment state (for binary operations)
    pub fn save_state(&self) -> Vec<HashMap<String, Value>> {
        self.frames.clone()
    }

    /// Restore environment state (for binary operations)
    pub fn restore_state(&mut self, state: Vec<HashMap<String, Value>>) {
        self.frames = state;
    }
}
