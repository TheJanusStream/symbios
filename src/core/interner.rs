use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone)]
pub struct SymbolTable {
    to_id: HashMap<Rc<str>, u16>,
    to_str: Vec<Rc<str>>,
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

    pub fn with_capacity(max_bytes: usize) -> Self {
        Self {
            to_id: HashMap::new(),
            to_str: Vec::new(),
            current_bytes: 0,
            max_bytes,
        }
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
        // 1. Fast check
        if let Some(&id) = self.to_id.get(name) {
            return Ok(id);
        }

        // 2. Safety checks
        if self.to_str.len() >= u16::MAX as usize {
            return Err("ID overflow".into());
        }
        if self.current_bytes + name.len() > self.max_bytes {
            return Err("Interner heap overflow".into());
        }

        let id = self.to_str.len() as u16;
        self.current_bytes += name.len();

        // 3. Single Allocation
        let rc_name: Rc<str> = Rc::from(name);

        // 4. Shared Ownership
        self.to_id.insert(rc_name.clone(), id);
        self.to_str.push(rc_name);

        Ok(id)
    }

    pub fn resolve_id(&self, name: &str) -> Option<u16> {
        self.to_id.get(name).copied()
    }

    pub fn resolve(&self, id: u16) -> Option<&str> {
        self.to_str.get(id as usize).map(|s| &**s)
    }

    pub fn len(&self) -> usize {
        self.to_str.len()
    }

    pub fn is_empty(&self) -> bool {
        self.to_str.is_empty()
    }
}

impl Serialize for SymbolTable {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("SymbolTable", 2)?;
        state.serialize_field("to_str", &self.to_str)?;
        state.serialize_field("max_bytes", &self.max_bytes)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for SymbolTable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Snapshot {
            to_str: Vec<String>,
            max_bytes: usize,
        }

        let snapshot = Snapshot::deserialize(deserializer)?;
        let mut table = SymbolTable::with_capacity(snapshot.max_bytes);

        for s in snapshot.to_str {
            table.intern(&s).map_err(serde::de::Error::custom)?;
        }

        Ok(table)
    }
}
