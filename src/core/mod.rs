use thiserror::Error;

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
    params: Vec<f64>,
    topology: Vec<u32>,
    offsets: Vec<u32>,
    lengths: Vec<u16>,
}

impl SymbiosState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            symbols: Vec::with_capacity(cap),
            params: Vec::with_capacity(cap),
            topology: Vec::with_capacity(cap),
            offsets: Vec::with_capacity(cap),
            lengths: Vec::with_capacity(cap),
        }
    }

    pub fn clear(&mut self) {
        self.symbols.clear();
        self.params.clear();
        self.topology.clear();
        self.offsets.clear();
        self.lengths.clear();
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn push(&mut self, symbol: u16, parameters: &[f64]) -> Result<(), SymbiosError> {
        if self.symbols.len() >= u32::MAX as usize - 1 {
            return Err(SymbiosError::CapacityOverflow);
        }
        if (self.params.len() + parameters.len()) >= u32::MAX as usize {
            return Err(SymbiosError::CapacityOverflow);
        }
        if parameters.len() > u16::MAX as usize {
            return Err(SymbiosError::ParameterOverflow(parameters.len(), u16::MAX));
        }

        self.symbols.push(symbol);
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

        for (i, &sym) in self.symbols.iter().enumerate() {
            if sym == open_sym || sym == close_sym {
                self.topology[i] = u32::MAX;
            }
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
        if start + len > self.params.len() {
            return None;
        }

        let skip = match self.topology[index] {
            u32::MAX => None,
            val if (val as usize) < self.symbols.len() => Some(val as usize),
            _ => None,
        };

        Some(ModuleView {
            sym: self.symbols[index],
            params: &self.params[start..start + len],
            skip_idx: skip,
        })
    }
}

#[derive(Debug)]
pub struct ModuleView<'a> {
    pub sym: u16,
    pub params: &'a [f64],
    pub skip_idx: Option<usize>,
}
