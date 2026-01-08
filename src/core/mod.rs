/// The internal representation of the L-System state.
/// Uses a Structure-of-Arrays (SoA) layout for cache locality and
/// to avoid memory fragmentation associated with `Vec<Module>` structs.
#[derive(Debug, Clone, Default)]
pub struct SymbiosState {
    /// The sequence of symbol identifiers.
    /// u16 allows for 65,535 unique symbol types in the alphabet.
    pub symbols: Vec<u16>,

    /// The Monolithic Parameter Arena.
    /// All parameters for all modules are packed contiguously here.
    /// Access is managed via `param_offset` and `param_len`.
    pub params: Vec<f32>,

    /// The Topology Skip-Table.
    /// Stores the index of the matching bracket (structural partner).
    /// Used for O(1) context skipping.
    /// For non-structural symbols, this can store self-index or 0.
    pub topology: Vec<u32>,

    /// Mapping to the Parameter Arena.
    /// `offsets[i]` = start index in `params` for symbol `i`.
    /// `lengths[i]` = number of parameters for symbol `i`.
    /// We use parallel vectors to keep the `symbols` vector extremely dense.
    pub offsets: Vec<u32>,
    pub lengths: Vec<u8>,
}

impl SymbiosState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears the buffers without deallocating memory (Reuse).
    pub fn clear(&mut self) {
        self.symbols.clear();
        self.params.clear();
        self.topology.clear();
        self.offsets.clear();
        self.lengths.clear();
    }

    /// Pushes a module onto the end of the state.
    pub fn push(&mut self, symbol: u16, parameters: &[f32]) {
        self.symbols.push(symbol);

        let start = self.params.len() as u32;
        self.offsets.push(start);
        self.lengths.push(parameters.len() as u8);

        self.params.extend_from_slice(parameters);

        // Topology is calculated in a separate pass,
        // but we push a placeholder to keep alignment.
        self.topology.push(0);
    }

    /// Returns a view of the module at index `i`.
    pub fn get_view(&self, index: usize) -> Option<ModuleView<'_>> {
        if index >= self.symbols.len() {
            return None;
        }

        let sym = self.symbols[index];
        let start = self.offsets[index] as usize;
        let len = self.lengths[index] as usize;

        // Safety: We control the push logic, so start/len are valid.
        // In a hot loop, we might use unchecked access, but get_view is a utility.
        let params = &self.params[start..start + len];
        let skip = self.topology[index] as usize;

        Some(ModuleView {
            sym,
            params,
            skip_idx: skip,
        })
    }
}

/// A temporary view into the SoA data, useful for higher-level logic.
/// This is NOT stored; it is constructed on the fly.
#[derive(Debug)]
pub struct ModuleView<'a> {
    pub sym: u16,
    pub params: &'a [f32],
    pub skip_idx: usize,
}
