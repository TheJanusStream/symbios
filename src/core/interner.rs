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

    pub fn intern(&mut self, name: &str) -> u16 {
        if let Some(&id) = self.to_id.get(name) {
            return id;
        }

        let id = self.to_str.len() as u16;
        // In a real scenario we might check for u16::MAX overflow here
        self.to_str.push(name.to_string());
        self.to_id.insert(name.to_string(), id);
        id
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
