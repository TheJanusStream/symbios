use crate::core::SymbiosState;
use crate::core::interner::SymbolTable;
use crate::parser::{self, ast};
use crate::vm::{Compiler, Op};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SystemError {
    #[error("Parser error: {0}")]
    ParseError(String),
    #[error("Compilation error: {0}")]
    CompileError(String),
    #[error("Invalid predecessor parameter")]
    InvalidPredecessorParam,
}

#[derive(Debug, Clone)]
pub struct RuntimeModule {
    pub symbol: u16,
    pub params: Vec<Vec<Op>>,
}

#[derive(Debug, Clone)]
pub struct RuntimeRule {
    pub predecessor: u16,
    pub left_context: Vec<u16>,
    pub right_context: Vec<u16>,
    pub probability: f64,
    pub condition: Option<Vec<Op>>,
    pub successors: Vec<RuntimeModule>,
    pub param_count: usize,
}

pub struct System {
    pub interner: SymbolTable,
    pub rules: Vec<RuntimeRule>,
    pub state: SymbiosState,
    pub ignored_symbols: Vec<u16>,
}

impl System {
    pub fn new() -> Self {
        Self {
            interner: SymbolTable::new(),
            rules: Vec::new(),
            state: SymbiosState::new(),
            ignored_symbols: Vec::new(),
        }
    }

    pub fn add_rule(&mut self, rule_src: &str) -> Result<(), SystemError> {
        let (_, rule_ast) =
            parser::parse_rule(rule_src).map_err(|e| SystemError::ParseError(e.to_string()))?;

        let mut param_names = Vec::new();
        for param in &rule_ast.predecessor.params {
            if let ast::Expr::Variable(name) = param {
                if param_names.contains(name) {
                    return Err(SystemError::CompileError(format!(
                        "Shadowing check failed: {}",
                        name
                    )));
                }
                param_names.push(name.clone());
            } else {
                return Err(SystemError::InvalidPredecessorParam);
            }
        }

        let mut compiler = Compiler::new(param_names);
        let pred_sym = self
            .interner
            .intern(&rule_ast.predecessor.symbol)
            .map_err(SystemError::CompileError)?;

        let mut left_ctx = Vec::new();
        for m in rule_ast.left_context {
            left_ctx.push(
                self.interner
                    .intern(&m.symbol)
                    .map_err(SystemError::CompileError)?,
            );
        }

        let mut right_ctx = Vec::new();
        for m in rule_ast.right_context {
            right_ctx.push(
                self.interner
                    .intern(&m.symbol)
                    .map_err(SystemError::CompileError)?,
            );
        }

        let condition_code = if let Some(ce) = &rule_ast.condition {
            Some(compiler.compile(ce).map_err(SystemError::CompileError)?)
        } else {
            None
        };

        let mut runtime_successors = Vec::new();
        for succ in &rule_ast.successors {
            let succ_sym = self
                .interner
                .intern(&succ.symbol)
                .map_err(SystemError::CompileError)?;
            let mut compiled_params = Vec::new();
            for expr in &succ.params {
                compiled_params.push(compiler.compile(expr).map_err(SystemError::CompileError)?);
            }
            runtime_successors.push(RuntimeModule {
                symbol: succ_sym,
                params: compiled_params,
            });
        }

        self.rules.push(RuntimeRule {
            predecessor: pred_sym,
            left_context: left_ctx,
            right_context: right_ctx,
            probability: rule_ast.probability,
            condition: condition_code,
            successors: runtime_successors,
            param_count: rule_ast.predecessor.params.len(),
        });
        Ok(())
    }

    pub fn set_axiom(&mut self, axiom_src: &str) -> Result<(), SystemError> {
        let mut remaining = axiom_src;
        self.state.clear();
        while !remaining.trim().is_empty() {
            let (ni, module) = parser::parse_module(remaining)
                .map_err(|e| SystemError::ParseError(e.to_string()))?;
            let sym_id = self
                .interner
                .intern(&module.symbol)
                .map_err(SystemError::CompileError)?;
            let mut values = Vec::new();
            for expr in module.params {
                if let ast::Expr::Number(v) = expr {
                    values.push(v);
                } else {
                    return Err(SystemError::CompileError("Axiom requires literals".into()));
                }
            }
            self.state
                .push(sym_id, 0.0, &values)
                .map_err(|e| SystemError::CompileError(e.to_string()))?;
            remaining = ni;
        }
        Ok(())
    }
}

pub mod matching {
    use crate::core::SymbiosState;
    use crate::system::SystemError;
    use crate::vm::{Op, VirtualMachine};

    /// Matches a rule's context requirements against the current state at `index`.
    pub fn matches(
        state: &SymbiosState,
        index: usize,
        predecessor_sym: u16,
        left_context: &[u16],
        right_context: &[u16],
        condition: Option<&[Op]>,
        ignore: &[u16],
        vm: &mut VirtualMachine,
    ) -> Result<bool, SystemError> {
        let view = state
            .get_view(index)
            .ok_or(SystemError::InvalidPredecessorParam)?;

        // 1. Match Predecessor
        if view.sym != predecessor_sym {
            return Ok(false);
        }

        // 2. Match Left Context
        if !left_context.is_empty() {
            if !match_left(state, index, left_context, ignore) {
                return Ok(false);
            }
        }

        // 3. Match Right Context
        if !right_context.is_empty() {
            if !match_right(state, index, right_context, ignore) {
                return Ok(false);
            }
        }

        // 4. Match Condition
        if let Some(code) = condition {
            let res = vm
                .eval(code, view.params)
                .map_err(|e| SystemError::CompileError(e))?;
            // IEEE-754: 0.0 is False, anything else is True
            if res == 0.0 {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Scans backwards, skipping branches to find the main axis ancestor.
    fn match_left(
        state: &SymbiosState,
        start_index: usize,
        pattern: &[u16],
        ignore: &[u16],
    ) -> bool {
        if start_index == 0 {
            return false;
        }
        let mut curr = start_index - 1;
        let mut pat_idx = pattern.len() - 1; // Match pattern right-to-left

        loop {
            let view = state.get_view(curr).unwrap(); // Safe: curr < start_index

            // A. Ignore Check
            if ignore.contains(&view.sym) {
                if curr == 0 {
                    return false;
                }
                curr -= 1;
                continue;
            }

            // B. Skip Logic (ABOP p. 32)
            // If we hit a closing bracket ']', we must skip the entire branch
            // using the topology link.
            if let Some(skip_target) = view.skip_idx {
                // If it's a closing bracket (topology link points backwards/smaller)
                if skip_target < curr {
                    curr = skip_target;
                    // Move one more step back to avoid processing the opening bracket
                    if curr == 0 {
                        return false;
                    }
                    curr -= 1;
                    continue;
                }
            }

            // C. Symbol Match
            if view.sym == pattern[pat_idx] {
                if pat_idx == 0 {
                    return true; // Match Complete
                }
                pat_idx -= 1;
            } else {
                return false; // Mismatch
            }

            if curr == 0 {
                return false;
            }
            curr -= 1;
        }
    }

    /// Scans forwards.
    fn match_right(
        state: &SymbiosState,
        start_index: usize,
        pattern: &[u16],
        ignore: &[u16],
    ) -> bool {
        let mut curr = start_index + 1;
        let mut pat_idx = 0;

        while curr < state.len() {
            let view = state.get_view(curr).unwrap();

            // A. Ignore Check
            if ignore.contains(&view.sym) {
                curr += 1;
                continue;
            }

            // B. Skip Logic
            // If we hit an opening bracket '[', we might need to enter it OR skip it
            // depending on standard L-system definitions.
            // Standard 2L logic: The main axis continues *past* the branch.
            // Branches are usually considered "Right Context" only if explicitly requested.
            // For standard "Signal Propagation", we usually skip OVER branches when
            // looking for the next segment on the main axis.
            if let Some(skip_target) = view.skip_idx {
                // If it's an opening bracket (link points forwards/larger)
                if skip_target > curr {
                    // Skip the branch content
                    curr = skip_target + 1;
                    continue;
                }
            }
            // Note: Closing brackets ']' are just ignored in forward scan of main axis

            // C. Symbol Match
            if view.sym == pattern[pat_idx] {
                pat_idx += 1;
                if pat_idx >= pattern.len() {
                    return true;
                }
            } else {
                return false;
            }
            curr += 1;
        }
        false
    }
}
