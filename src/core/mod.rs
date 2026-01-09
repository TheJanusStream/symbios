use thiserror::Error;
pub mod interner;

#[derive(Error, Debug, PartialEq)]
pub enum SymbiosError {
    #[error("Parameter count {0} exceeds limit {1}")]
    ParameterOverflow(usize, u16),
    #[error("Unmatched bracket at index {0}")]
    UnmatchedBracket(usize),
    #[error("Ambiguous topology symbols")]
    AmbiguousTopology,
    #[error("Max nesting depth exceeded")]
    MaxNestingExceeded,
    #[error("State capacity overflow")]
    CapacityOverflow,
}

#[derive(Debug, Clone)]
struct ModuleData {
    symbol: u16,
    birth_time: f64, // Absolute time for O(1) advance_time
    param_start: u32,
    param_len: u16,
    topology_link: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SymbiosState {
    modules: Vec<ModuleData>,
    params: Vec<f64>,
    pub current_time: f64,
}

impl SymbiosState {
    const NO_LINK: u32 = u32::MAX;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.modules.clear();
        self.params.clear();
        self.current_time = 0.0;
    }

    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    pub fn push(&mut self, symbol: u16, age: f64, parameters: &[f64]) -> Result<(), SymbiosError> {
        if self.modules.len() >= (u32::MAX as usize - 1)
            || (self.params.len() + parameters.len()) >= (u32::MAX as usize - 1)
        {
            return Err(SymbiosError::CapacityOverflow);
        }

        if parameters.len() > u16::MAX as usize {
            return Err(SymbiosError::ParameterOverflow(parameters.len(), u16::MAX));
        }

        let param_start = self.params.len() as u32;
        self.params.extend_from_slice(parameters);

        self.modules.push(ModuleData {
            symbol,
            birth_time: self.current_time - age,
            param_start,
            param_len: parameters.len() as u16,
            topology_link: Self::NO_LINK,
        });
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
        for i in 0..self.modules.len() {
            let sym = self.modules[i].symbol;
            if sym == open_sym {
                if stack.len() >= 4096 {
                    return Err(SymbiosError::MaxNestingExceeded);
                }
                stack.push(i as u32);
            } else if sym == close_sym {
                if let Some(si) = stack.pop() {
                    self.modules[si as usize].topology_link = i as u32;
                    self.modules[i].topology_link = si;
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
        let m = self.modules.get(index)?;
        let start = m.param_start as usize;
        let end = start + (m.param_len as usize);

        let skip = if m.topology_link == Self::NO_LINK {
            None
        } else {
            Some(m.topology_link as usize)
        };

        Some(ModuleView {
            sym: m.symbol,
            age: self.current_time - m.birth_time,
            params: &self.params[start..end],
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
