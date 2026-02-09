use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SymbolTable {
    to_id: HashMap<Arc<str>, u16>,
    to_str: Vec<Arc<str>>,
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
        let arc_name: Arc<str> = Arc::from(name);

        // 4. Shared Ownership
        self.to_id.insert(arc_name.clone(), id);
        self.to_str.push(arc_name);

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

    /// Returns an iterator over (id, name) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &str)> {
        self.to_str
            .iter()
            .enumerate()
            .map(|(id, name)| (id as u16, &**name))
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
        use serde::de::{self, MapAccess, Visitor};

        // Stream-validate: deserialize fields individually so we can check
        // max_bytes before allocating the full string table.
        struct SymbolTableVisitor;

        impl<'de> Visitor<'de> for SymbolTableVisitor {
            type Value = SymbolTable;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a SymbolTable struct with to_str and max_bytes fields")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut to_str: Option<Vec<String>> = None;
                let mut max_bytes: Option<usize> = None;

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "to_str" => to_str = Some(map.next_value()?),
                        "max_bytes" => max_bytes = Some(map.next_value()?),
                        _ => {
                            let _ = map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }

                let strings = to_str.ok_or_else(|| de::Error::missing_field("to_str"))?;
                let max = max_bytes.ok_or_else(|| de::Error::missing_field("max_bytes"))?;

                // Validate entry count before interning (u16 ID space)
                if strings.len() > u16::MAX as usize {
                    return Err(de::Error::custom(format!(
                        "Symbol table has {} entries, exceeds u16::MAX ({})",
                        strings.len(),
                        u16::MAX
                    )));
                }

                // Validate total byte budget before interning
                let total_bytes: usize = strings.iter().map(|s| s.len()).sum();
                if total_bytes > max {
                    return Err(de::Error::custom(format!(
                        "Total string data ({} bytes) exceeds max_bytes ({})",
                        total_bytes, max
                    )));
                }

                let mut table = SymbolTable::with_capacity(max);
                for s in strings {
                    table.intern(&s).map_err(de::Error::custom)?;
                }
                Ok(table)
            }
        }

        deserializer.deserialize_struct("SymbolTable", &["to_str", "max_bytes"], SymbolTableVisitor)
    }
}
