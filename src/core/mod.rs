use thiserror::Error;

pub mod interner;

#[derive(Error, Debug, PartialEq)]
pub enum SymbiosError {
    #[error("Parameter count {0} exceeds limit {1}")]
    ParameterOverflow(usize, u16),
    #[error("Unmatched bracket at index {0}")]
    UnmatchedBracket(usize),
    #[error("Ambiguous topology symbols: open and close are identical")]
    AmbiguousTopology,
    #[error("Max nesting depth exceeded")]
    MaxNestingExceeded,
    #[error("State capacity overflow")]
    CapacityOverflow,
}

#[derive(Debug, Clone, Default)]
pub struct SymbiosState {
    symbols: Vec<u16>,
    birth_times: Vec<f64>,
    params: Vec<f64>,
    topology: Vec<u32>,
    offsets: Vec<u32>,
    lengths: Vec<u16>,
    pub current_time: f64,
}

impl SymbiosState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.symbols.clear();
        self.birth_times.clear();
        self.params.clear();
        self.topology.clear();
        self.offsets.clear();
        self.lengths.clear();
        self.current_time = 0.0;
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn push(&mut self, symbol: u16, age: f64, parameters: &[f64]) -> Result<(), SymbiosError> {
        const SAFE_LIMIT: usize = (u32::MAX as usize) - 1024;
        if self.symbols.len() >= SAFE_LIMIT || (self.params.len() + parameters.len()) >= SAFE_LIMIT
        {
            return Err(SymbiosError::CapacityOverflow);
        }
        if parameters.len() > u16::MAX as usize {
            return Err(SymbiosError::ParameterOverflow(parameters.len(), u16::MAX));
        }

        self.symbols.push(symbol);
        self.birth_times.push(self.current_time - age);
        self.offsets.push(self.params.len() as u32);
        self.lengths.push(parameters.len() as u16);
        self.params.extend_from_slice(parameters);
        self.topology.push(u32::MAX);
        Ok(())
    }

    pub fn calculate_topology(
        &mut self,
        open_sym: u16,
        close_sym: u16,
    ) -> Result<(), SymbiosError> {
        if open_sym == close_sym {
            return Err(SymbiosError::AmbiguousTopology);
        }
        let mut stack = Vec::new();
        const MAX_NESTING: usize = 4096;

        for (i, &sym) in self.symbols.iter().enumerate() {
            if sym == open_sym {
                if stack.len() >= MAX_NESTING {
                    return Err(SymbiosError::MaxNestingExceeded);
                }
                stack.push(i as u32);
            } else if sym == close_sym {
                if let Some(start_idx) = stack.pop() {
                    self.topology[start_idx as usize] = i as u32;
                    self.topology[i] = start_idx;
                } else {
                    return Err(SymbiosError::UnmatchedBracket(i));
                }
            }
        }
        if !stack.is_empty() {
            return Err(SymbiosError::UnmatchedBracket(stack[0] as usize));
        }
        Ok(())
    }

    pub fn get_view(&self, index: usize) -> Option<ModuleView<'_>> {
        if index >= self.symbols.len() {
            return None;
        }
        let start = self.offsets[index] as usize;
        let len = self.lengths[index] as usize;
        let skip = match self.topology[index] {
            u32::MAX => None,
            val if (val as usize) < self.symbols.len() => Some(val as usize),
            _ => None,
        };

        Some(ModuleView {
            sym: self.symbols[index],
            age: self.current_time - self.birth_times[index],
            params: &self.params[start..start + len],
            skip_idx: skip,
        })
    }

    pub fn advance_time(&mut self, dt: f64) {
        self.current_time += dt;
    }
}

#[derive(Debug)]
pub struct ModuleView<'a> {
    pub sym: u16,
    pub age: f64,
    pub params: &'a [f64],
    pub skip_idx: Option<usize>,
}
