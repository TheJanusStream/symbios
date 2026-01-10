use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolTable {
    to_id: HashMap<String, u16>,
    to_str: Vec<String>,
    current_bytes: usize,
    max_bytes: usize,
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::with_capacity(10 * 1024 * 1024)
    }
}

impl SymbolTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Interns a string and returns a unique u16 ID.
    /// Returns error on overflow (DoS protection).
    pub fn get_or_intern(&mut self, name: &str) -> Result<u16, String> {
        if let Some(&id) = self.to_id.get(name) {
            return Ok(id);
        }
        self.intern(name)
    }

    pub fn with_capacity(max_bytes: usize) -> Self {
        Self {
            to_id: HashMap::new(),
            to_str: Vec::new(),
            current_bytes: 0,
            max_bytes,
        }
    }

    pub fn intern(&mut self, name: &str) -> Result<u16, String> {
        if let Some(&id) = self.to_id.get(name) {
            return Ok(id);
        }

        if self.to_str.len() >= u16::MAX as usize {
            return Err("ID overflow".into());
        }
        if self.current_bytes + name.len() > self.max_bytes {
            return Err("Interner heap overflow".into());
        }

        let id = self.to_str.len() as u16;
        self.current_bytes += name.len();
        self.to_id.insert(name.to_string(), id);
        self.to_str.push(name.to_string());
        Ok(id)
    }

    pub fn resolve_id(&self, name: &str) -> Option<u16> {
        self.to_id.get(name).copied()
    }

    pub fn resolve(&self, id: u16) -> Option<&str> {
        self.to_str.get(id as usize).map(|s| s.as_str())
    }

    pub fn len(&self) -> usize {
        self.to_str.len()
    }

    pub fn is_empty(&self) -> bool {
        self.to_str.is_empty()
    }
}
