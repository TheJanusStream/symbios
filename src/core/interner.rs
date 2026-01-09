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
    /// Returns an error if the table exceeds 65535 unique symbols.
    pub fn intern(&mut self, name: &str) -> Result<u16, String> {
        // Use entry to avoid double-lookup and unnecessary to_string()
        if let Some(&id) = self.to_id.get(name) {
            return Ok(id);
        }

        if self.to_str.len() >= u16::MAX as usize {
            return Err("Symbol table overflow: maximum of 65535 symbols reached".to_string());
        }

        let id = self.to_str.len() as u16;
        let name_string = name.to_string();
        self.to_str.push(name_string.clone());
        self.to_id.insert(name_string, id);

        Ok(id)
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
