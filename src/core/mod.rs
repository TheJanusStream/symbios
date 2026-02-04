use std::fmt;

use thiserror::Error;
pub mod interner;
use serde::{Deserialize, Serialize};

use crate::SymbolTable;

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
    #[error("Internal index out of bounds: {0}")]
    InvalidIndex(usize),
    #[error("Non-finite numeric value detected (NaN/Inf)")]
    InvalidNumericValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleData {
    symbol: u16,
    birth_time: f64, // Absolute time for O(1) advance_time
    param_start: u32,
    param_len: u16,
    topology_link: u32,
}

/// Represents the current state of the L-System simulation.
///
/// It stores the linear sequence of modules using a Structure-of-Arrays (SoA) layout
/// to maximize cache locality and minimize allocation overhead.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbiosState {
    modules: Vec<ModuleData>,
    params: Vec<f64>,
    pub current_time: f64,
    pub max_capacity: usize,
}

impl Default for SymbiosState {
    fn default() -> Self {
        Self {
            modules: Vec::new(),
            params: Vec::new(),
            current_time: 0.0,
            max_capacity: 1_000_000,
        }
    }
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

    /// Returns the number of modules in the current string.
    pub fn len(&self) -> usize {
        self.modules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Appends a new module to the state.
    pub fn push(&mut self, symbol: u16, age: f64, parameters: &[f64]) -> Result<(), SymbiosError> {
        if !age.is_finite() || parameters.iter().any(|p| !p.is_finite()) {
            return Err(SymbiosError::InvalidNumericValue);
        }

        if self.modules.len() >= self.max_capacity {
            return Err(SymbiosError::CapacityOverflow);
        }

        // Use checked arithmetic to prevent overflow on 32-bit architectures
        let new_params_len = self
            .params
            .len()
            .checked_add(parameters.len())
            .ok_or(SymbiosError::CapacityOverflow)?;

        if self.modules.len() >= (u32::MAX as usize - 1)
            || new_params_len >= (u32::MAX as usize - 1)
        {
            return Err(SymbiosError::CapacityOverflow);
        }

        if parameters.len() > u16::MAX as usize {
            return Err(SymbiosError::ParameterOverflow(parameters.len(), u16::MAX));
        }

        // Explicit truncation guard: ensure params.len() fits in u32 before cast
        // (defense in depth against any bypassed capacity checks)
        if self.params.len() > u32::MAX as usize {
            return Err(SymbiosError::CapacityOverflow);
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

    /// Pre-calculates skip-links for branching structures.
    ///
    /// This enables O(1) context matching over branches.
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

    /// Retrieves a read-only view of a specific module.
    pub fn get_view(&self, index: usize) -> Option<ModuleView<'_>> {
        let m = self.modules.get(index)?;
        let start = m.param_start as usize;
        let end = start + (m.param_len as usize);

        let skip = if m.topology_link == Self::NO_LINK {
            None
        } else {
            Some(m.topology_link as usize)
        };

        let params = self.params.get(start..end)?;

        Some(ModuleView {
            sym: m.symbol,
            age: self.current_time - m.birth_time,
            params,
            skip_idx: skip,
        })
    }

    pub fn advance_time(&mut self, dt: f64) -> Result<(), String> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(format!("Invalid time step: {}", dt));
        }
        let new_time = self.current_time + dt;
        // Prevent overflow to infinity which would permanently brick the system
        // (push() rejects non-finite birth_time values)
        if !new_time.is_finite() {
            return Err(format!(
                "Time overflow: {} + {} exceeds representable range",
                self.current_time, dt
            ));
        }
        self.current_time = new_time;
        Ok(())
    }

    /// Returns a helper struct for formatting the state as a string.
    pub fn display<'a>(&'a self, interner: &'a SymbolTable) -> StateDisplay<'a> {
        StateDisplay {
            state: self,
            interner,
        }
    }
}

pub struct StateDisplay<'a> {
    state: &'a SymbiosState,
    interner: &'a SymbolTable,
}

impl<'a> fmt::Display for StateDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for i in 0..self.state.len() {
            if i > 0 {
                write!(f, " ")?;
            }
            if let Some(view) = self.state.get_view(i) {
                let sym_str = self.interner.resolve(view.sym).unwrap_or("?");
                write!(f, "{}", sym_str)?;

                if !view.params.is_empty() {
                    write!(f, "(")?;
                    for (j, param) in view.params.iter().enumerate() {
                        if j > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{:.4}", param)?;
                    }
                    write!(f, ")")?;
                }
            } else {
                write!(f, "ERR_VIEW")?;
            }
        }
        Ok(())
    }
}

/// A temporary view into a module's data.
#[derive(Debug)]
pub struct ModuleView<'a> {
    pub sym: u16,
    pub age: f64,
    pub params: &'a [f64],
    pub skip_idx: Option<usize>,
}
