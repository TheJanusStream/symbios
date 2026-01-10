use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    to_id: HashMap<String, u16>,
    to_str: Vec<String>,
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

    pub fn intern(&mut self, name: &str) -> Result<u16, String> {
        if let Some(&id) = self.to_id.get(name) {
            return Ok(id);
        }

        if self.to_str.len() >= u16::MAX as usize {
            return Err("Symbol table overflow".into());
        }

        let id = self.to_str.len() as u16;
        let name_owned = name.to_string();

        self.to_id.insert(name_owned.clone(), id);
        self.to_str.push(name_owned);
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
