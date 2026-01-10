use crate::core::SymbiosState;
use crate::core::interner::SymbolTable;
use crate::parser::{self, ast};
use crate::vm::{Compiler, Op};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SystemError {
    #[error("Parser error: {0}")]
    ParseError(String),
    #[error("Compilation error: {0}")]
    CompileError(String),
    #[error("Invalid predecessor parameter")]
    InvalidPredecessorParam,
    #[error("Interner error: {0}")]
    InternerError(String),
    #[error("VM error: {0}")]
    VMError(String),
    #[error("State error: {0}")]
    StateError(String),
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
    // Arity Enforcement: Store expected param count for [Pred, L1, L2..., R1, R2...]
    // The Compiler assumes this exact layout.
    pub expected_arities: Vec<usize>,
}

pub struct System {
    pub interner: SymbolTable,
    pub rules: Vec<RuntimeRule>,
    pub state: SymbiosState,
    pub ignored_symbols: Vec<u16>,
    pub rng: Pcg64,
}

impl System {
    pub fn new() -> Self {
        Self {
            interner: SymbolTable::new(),
            rules: Vec::new(),
            state: SymbiosState::new(),
            ignored_symbols: Vec::new(),
            rng: Pcg64::seed_from_u64(42), // Default seed for reproducibility
        }
    }

    pub fn set_seed(&mut self, seed: u64) {
        self.rng = Pcg64::seed_from_u64(seed);
    }

    pub fn derive(&mut self, steps: usize) -> Result<(), SystemError> {
        let mut vm = crate::vm::VirtualMachine::new();

        let open_sym = self.interner.resolve_id("[");
        let close_sym = self.interner.resolve_id("]");

        for _ in 0..steps {
            if let (Some(o), Some(c)) = (open_sym, close_sym) {
                self.state
                    .calculate_topology(o, c)
                    .map_err(|e| SystemError::StateError(e.to_string()))?;
            }

            let mut next_state = SymbiosState::new();
            next_state.current_time = self.state.current_time;

            for index in 0..self.state.len() {
                let view = self
                    .state
                    .get_view(index)
                    .ok_or(SystemError::StateError("Index out of bounds".to_string()))?;

                // 1. Collect ALL matching candidates
                let mut candidates = Vec::new();
                let mut total_probability = 0.0;

                for rule in &self.rules {
                    if view.sym != rule.predecessor {
                        continue;
                    }

                    // This is the expensive part (VM execution for guards),
                    // but necessary for stochastic + context/parametric systems.
                    let is_match =
                        matching::matches(&self.state, index, rule, &self.ignored_symbols, &mut vm)
                            .map_err(|e| SystemError::StateError(e.to_string()))?;

                    if is_match {
                        candidates.push(rule);
                        total_probability += rule.probability;
                    }
                }

                // 2. Select Winner
                let selected_rule = if candidates.is_empty() {
                    None
                } else if candidates.len() == 1 {
                    Some(candidates[0])
                } else {
                    // Weighted Random Choice (rand 0.8 syntax)
                    let mut r = self.rng.random_range(0.0..total_probability);
                    let mut winner = None;

                    // FIX: Iterate by reference to avoid moving `candidates`
                    for candidate in &candidates {
                        if r < candidate.probability {
                            winner = Some(*candidate);
                            break;
                        }
                        r -= candidate.probability;
                    }
                    // Fallback for floating point errors (pick last)
                    winner.or_else(|| candidates.last().copied())
                };

                // 3. Apply
                if let Some(rule) = selected_rule {
                    // (Context collection logic same as before)
                    let mut context_frame = Vec::new();
                    context_frame.extend_from_slice(view.params);

                    let mut left_indices = Vec::new();
                    if !rule.left_context.is_empty() {
                        matching::match_left(
                            &self.state,
                            index,
                            &rule.left_context,
                            &self.ignored_symbols,
                            &mut left_indices,
                        );
                    }
                    for &i in &left_indices {
                        context_frame.extend_from_slice(self.state.get_view(i).unwrap().params);
                    }

                    let mut right_indices = Vec::new();
                    if !rule.right_context.is_empty() {
                        matching::match_right(
                            &self.state,
                            index,
                            &rule.right_context,
                            &self.ignored_symbols,
                            &mut right_indices,
                        );
                    }
                    for &i in &right_indices {
                        context_frame.extend_from_slice(self.state.get_view(i).unwrap().params);
                    }

                    for successor in &rule.successors {
                        let mut new_params = Vec::new();
                        for param_code in &successor.params {
                            let val = vm
                                .eval(param_code, &context_frame, view.age)
                                .map_err(SystemError::VMError)?;
                            new_params.push(val);
                        }

                        next_state
                            .push(successor.symbol, 0.0, &new_params)
                            .map_err(|e| SystemError::StateError(e.to_string()))?;
                    }
                } else {
                    // Identity
                    next_state
                        .push(view.sym, view.age, view.params)
                        .map_err(|e| SystemError::StateError(e.to_string()))?;
                }
            }

            self.state = next_state;
        }

        Ok(())
    }

    pub fn add_rule(&mut self, rule_src: &str) -> Result<(), SystemError> {
        let (_, rule_ast) =
            parser::parse_rule(rule_src).map_err(|e| SystemError::ParseError(e.to_string()))?;

        let mut param_names = Vec::new();
        let mut expected_arities = Vec::new();

        // 1. Predecessor
        expected_arities.push(rule_ast.predecessor.params.len());
        for param in &rule_ast.predecessor.params {
            if let ast::Expr::Variable(name) = param {
                if param_names.contains(name) {
                    return Err(SystemError::CompileError(format!("Shadowing: {}", name)));
                }
                param_names.push(name.clone());
            } else {
                return Err(SystemError::InvalidPredecessorParam);
            }
        }

        // 2. Left Context
        for m in &rule_ast.left_context {
            expected_arities.push(m.params.len());
            for param in &m.params {
                if let ast::Expr::Variable(name) = param {
                    param_names.push(name.clone());
                }
            }
        }

        // 3. Right Context
        for m in &rule_ast.right_context {
            expected_arities.push(m.params.len());
            for param in &m.params {
                if let ast::Expr::Variable(name) = param {
                    param_names.push(name.clone());
                }
            }
        }

        let mut compiler = Compiler::new(param_names);

        let pred_sym = self
            .interner
            .get_or_intern(&rule_ast.predecessor.symbol)
            .map_err(SystemError::InternerError)?;

        let mut left_ctx = Vec::new();
        for m in rule_ast.left_context {
            left_ctx.push(
                self.interner
                    .get_or_intern(&m.symbol)
                    .map_err(SystemError::InternerError)?,
            );
        }

        let mut right_ctx = Vec::new();
        for m in rule_ast.right_context {
            right_ctx.push(
                self.interner
                    .get_or_intern(&m.symbol)
                    .map_err(SystemError::InternerError)?,
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
                .get_or_intern(&succ.symbol)
                .map_err(SystemError::InternerError)?;
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
            expected_arities, // Stored for runtime check
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
                .get_or_intern(&module.symbol)
                .map_err(SystemError::InternerError)?;
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
    use crate::system::{RuntimeRule, SystemError};
    use crate::vm::VirtualMachine;

    pub fn matches(
        state: &SymbiosState,
        index: usize,
        rule: &RuntimeRule,
        ignore: &[u16],
        vm: &mut VirtualMachine,
    ) -> Result<bool, SystemError> {
        let pred_view = state
            .get_view(index)
            .ok_or(SystemError::InvalidPredecessorParam)?;

        // 1. Symbol Match
        if pred_view.sym != rule.predecessor {
            return Ok(false);
        }

        // 2. Arity Match (Predecessor) - CRITICAL FIX for Alignment
        if pred_view.params.len() != rule.expected_arities[0] {
            return Ok(false);
        }

        // 3. Context Match & Collect
        let mut left_indices = Vec::new();
        if !rule.left_context.is_empty() {
            if !match_left(state, index, &rule.left_context, ignore, &mut left_indices) {
                return Ok(false);
            }
        }

        let mut right_indices = Vec::new();
        if !rule.right_context.is_empty() {
            if !match_right(
                state,
                index,
                &rule.right_context,
                ignore,
                &mut right_indices,
            ) {
                return Ok(false);
            }
        }

        // 4. Arity Match (Context)
        // Verify that every found context neighbor has the exact param count expected by the rule.
        // Left context arities start at index 1.
        for (i, &ctx_idx) in left_indices.iter().enumerate() {
            let view = state
                .get_view(ctx_idx)
                .ok_or(SystemError::InvalidPredecessorParam)?;
            if view.params.len() != rule.expected_arities[1 + i] {
                return Ok(false);
            }
        }
        // Right context arities start after Left.
        let right_offset = 1 + rule.left_context.len();
        for (i, &ctx_idx) in right_indices.iter().enumerate() {
            let view = state
                .get_view(ctx_idx)
                .ok_or(SystemError::InvalidPredecessorParam)?;
            if view.params.len() != rule.expected_arities[right_offset + i] {
                return Ok(false);
            }
        }

        // 5. Condition Eval
        if let Some(code) = &rule.condition {
            // Build Context Frame: Pred -> Left -> Right
            // Only strictly valid due to Arity Checks above.
            let mut context_frame = Vec::new();
            context_frame.extend_from_slice(pred_view.params);

            for &i in &left_indices {
                // Safe unwrap: verified in step 4
                context_frame.extend_from_slice(state.get_view(i).unwrap().params);
            }
            for &i in &right_indices {
                context_frame.extend_from_slice(state.get_view(i).unwrap().params);
            }

            let res = vm
                .eval(code, &context_frame, pred_view.age)
                .map_err(|e| SystemError::CompileError(e))?;

            if res == 0.0 {
                return Ok(false);
            }
        }

        Ok(true)
    }

    pub fn match_left(
        state: &SymbiosState,
        start_index: usize,
        pattern: &[u16],
        ignore: &[u16],
        matched_indices: &mut Vec<usize>,
    ) -> bool {
        if start_index == 0 {
            return false;
        }
        let mut curr = start_index - 1;
        let mut pat_idx = pattern.len() - 1;

        loop {
            // SAFE: Remove unwrap
            let view = match state.get_view(curr) {
                Some(v) => v,
                None => return false, // Should be impossible if index logic is sound, but safe
            };

            if ignore.contains(&view.sym) {
                if curr == 0 {
                    return false;
                }
                curr -= 1;
                continue;
            }

            if let Some(skip_target) = view.skip_idx {
                if skip_target < curr {
                    curr = skip_target;
                    if curr == 0 {
                        return false;
                    }
                    curr -= 1;
                    continue;
                }
            }

            if view.sym == pattern[pat_idx] {
                matched_indices.push(curr);
                if pat_idx == 0 {
                    matched_indices.reverse();
                    return true;
                }
                pat_idx -= 1;
            } else {
                return false;
            }

            if curr == 0 {
                return false;
            }
            curr -= 1;
        }
    }

    pub fn match_right(
        state: &SymbiosState,
        start_index: usize,
        pattern: &[u16],
        ignore: &[u16],
        matched_indices: &mut Vec<usize>,
    ) -> bool {
        let mut curr = start_index + 1;
        let mut pat_idx = 0;

        while curr < state.len() {
            let view = match state.get_view(curr) {
                Some(v) => v,
                None => return false,
            };

            if ignore.contains(&view.sym) {
                curr += 1;
                continue;
            }

            if view.sym == pattern[pat_idx] {
                matched_indices.push(curr);
                pat_idx += 1;
                if pat_idx >= pattern.len() {
                    return true;
                }
                curr += 1;
                continue;
            }

            if let Some(skip_target) = view.skip_idx {
                if skip_target > curr {
                    curr = skip_target + 1;
                    continue;
                }
            }

            return false;
        }
        false
    }
}
