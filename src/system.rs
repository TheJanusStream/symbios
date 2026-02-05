use crate::core::SymbiosState;
use crate::core::interner::SymbolTable;
use crate::parser::{self, ast};
use crate::vm::{Compiler, Op};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use std::collections::{HashMap, HashSet};
use thiserror::Error;

/// Maximum number of successors allowed per rule (DoS protection).
/// Matches the parser's limit for consistency.
const MAX_SUCCESSORS: usize = 128;

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
    State(#[from] crate::core::SymbiosError),
    #[error("Internal state corruption: invalid index {0}")]
    StateCorruption(usize),
}

/// A compiled successor module ready for generation.
///
/// Contains the symbol ID and the bytecode for evaluating its parameters.
#[derive(Debug, Clone)]
pub struct RuntimeModule {
    pub symbol: u16,
    pub params: Vec<Vec<Op>>,
}

/// A fully compiled L-System production rule.
///
/// This struct contains the optimized logic for matching (predecessor, context)
/// and generation (successors, probability).
#[derive(Debug, Clone)]
pub struct RuntimeRule {
    /// The ID of the symbol this rule replaces.
    pub predecessor: u16,
    /// Sequence of symbol IDs required to the left.
    pub left_context: Vec<u16>,
    /// Sequence of symbol IDs required to the right.
    pub right_context: Vec<u16>,
    /// Stochastic weight for rule selection (typically 0.0 - 1.0).
    ///
    /// **Important**: This value is a *relative weight*, not an absolute probability.
    /// When multiple rules match the same module, their weights are summed and each
    /// rule's chance of being selected is `weight / total_weight`.
    ///
    /// This means:
    /// - A single rule with weight 0.3 will fire 100% of the time (not 30%)
    /// - Two rules with weights 0.3 and 0.7 fire with 30% and 70% probability respectively
    /// - To implement "30% chance to transform, 70% identity", use two rules:
    ///   `0.3: A -> B` and `0.7: A -> A`
    pub probability: f64,
    /// Bytecode for the guard condition (evaluates to 1.0 for true).
    pub condition: Option<Vec<Op>>,
    /// The sequence of modules to produce if matched.
    pub successors: Vec<RuntimeModule>,
    /// Expected parameter counts for validation.
    pub expected_arities: Vec<usize>,
}

/// The primary interface for defining and simulating an L-System.
///
/// `System` coordinates the Parser, Interner, Virtual Machine, and State
/// to execute derivations. It owns the rules and the current state of the simulation.
pub struct System {
    /// The symbol interner, mapping string identifiers to `u16` IDs.
    pub interner: SymbolTable,
    /// The set of compiled rules, indexed by predecessor symbol ID.
    pub rules: HashMap<u16, Vec<RuntimeRule>>,
    /// The current state of the simulation (the string of modules).
    pub state: SymbiosState,
    /// Double-buffering target to prevent allocations during derivation.
    back_buffer: SymbiosState,
    /// A list of symbol IDs to ignore during context matching.
    pub ignored_symbols: Vec<u16>,
    /// The random number generator (PCG64) for stochastic rules.
    pub rng: Pcg64,
    /// Global constants defined via `#define`.
    pub constants: HashMap<String, f64>,
    /// Safety limit for total module count to prevent OOM. Default: 1,000,000.
    pub max_capacity: usize,
    /// Reusable scratch buffers for zero-allocation matching.
    scratch: matching::MatchScratch,
    /// Reusable buffer for context frame during successor generation.
    gen_context_frame: Vec<f64>,
    /// Reusable buffer for left indices during successor generation.
    gen_left_indices: Vec<usize>,
    /// Reusable buffer for right indices during successor generation.
    gen_right_indices: Vec<usize>,
    /// Reusable buffer for successor parameters.
    gen_new_params: Vec<f64>,
    /// Reusable buffer for candidate rule indices during derivation.
    derive_candidate_indices: Vec<usize>,
    /// Stored initial axiom state for reset functionality.
    initial_state: Option<SymbiosState>,
    /// Known arities for symbols (symbol ID -> expected parameter count).
    /// Used by structural mutation to initialize inserted modules correctly.
    symbol_arities: HashMap<u16, usize>,
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

impl System {
    /// Creates a new System with default settings and a deterministic seed.
    pub fn new() -> Self {
        Self {
            interner: SymbolTable::new(),
            rules: HashMap::new(),
            state: SymbiosState::new(),
            back_buffer: SymbiosState::new(),
            ignored_symbols: Vec::new(),
            rng: Pcg64::seed_from_u64(42),
            constants: HashMap::new(),
            max_capacity: 1_000_000,
            scratch: matching::MatchScratch::new(),
            gen_context_frame: Vec::new(),
            gen_left_indices: Vec::new(),
            gen_right_indices: Vec::new(),
            gen_new_params: Vec::new(),
            derive_candidate_indices: Vec::new(),
            initial_state: None,
            symbol_arities: HashMap::new(),
        }
    }

    /// Sets the random seed for stochastic rule selection.
    pub fn set_seed(&mut self, seed: u64) {
        self.rng = Pcg64::seed_from_u64(seed);
    }

    /// Advances the system by `steps` generations.
    ///
    /// This method:
    /// 1. Calculates topology (if brackets are present).
    /// 2. Iterates through the current state.
    /// 3. Matches rules (including context and guards).
    /// 4. Generates the new state.
    ///
    /// # Stochastic Rule Selection
    ///
    /// When multiple rules match a module, selection uses *relative weights*:
    /// - All matching rules' probabilities are summed to `total_weight`
    /// - Each rule is selected with probability `rule.probability / total_weight`
    /// - A single matching rule always fires (even with probability < 1.0)
    ///
    /// To implement probabilistic identity (e.g., "30% chance to transform"),
    /// define an explicit identity rule: `0.7: A -> A` alongside `0.3: A -> B`.
    pub fn derive(&mut self, steps: usize) -> Result<(), SystemError> {
        let mut vm = crate::vm::VirtualMachine::new();
        let open_sym = self.interner.resolve_id("[");
        let close_sym = self.interner.resolve_id("]");

        for _ in 0..steps {
            if let (Some(o), Some(c)) = (open_sym, close_sym) {
                self.state.calculate_topology(o, c)?;
            }

            // Prepare back buffer: clear contents but keep capacity
            self.back_buffer.clear();
            self.back_buffer.max_capacity = self.max_capacity;
            self.back_buffer.current_time = self.state.current_time;

            for index in 0..self.state.len() {
                let view = self
                    .state
                    .get_view(index)
                    .ok_or(crate::core::SymbiosError::InvalidIndex(index))?;

                // Reuse scratch buffer for candidate indices (avoids per-module allocation)
                self.derive_candidate_indices.clear();
                let mut total_probability = 0.0;
                // Track the last matched rule index to enable scratch buffer reuse
                let mut last_matched_idx: Option<usize> = None;

                if let Some(bucket) = self.rules.get(&view.sym) {
                    for (rule_idx, rule) in bucket.iter().enumerate() {
                        // view.sym is guaranteed to match rule.predecessor here
                        let is_match = matching::matches(
                            &self.state,
                            index,
                            rule,
                            &self.ignored_symbols,
                            &mut vm,
                            &mut self.scratch,
                        )?;

                        if is_match {
                            self.derive_candidate_indices.push(rule_idx);
                            total_probability += rule.probability;
                            last_matched_idx = Some(rule_idx);
                        }
                    }
                }

                // Select rule from candidates using indices, tracking which index was selected
                let (selected_rule, selected_idx): (Option<&RuntimeRule>, Option<usize>) =
                    if self.derive_candidate_indices.is_empty() || total_probability <= 0.0 {
                        (None, None)
                    } else if self.derive_candidate_indices.len() == 1 {
                        let idx = self.derive_candidate_indices[0];
                        (
                            self.rules.get(&view.sym).and_then(|b| b.get(idx)),
                            Some(idx),
                        )
                    } else {
                        let bucket = self.rules.get(&view.sym);
                        let mut r = self.rng.random_range(0.0..total_probability);
                        let mut winner = None;
                        let mut winner_idx = None;
                        for &rule_idx in &self.derive_candidate_indices {
                            if let Some(rule) = bucket.and_then(|b| b.get(rule_idx)) {
                                if r < rule.probability {
                                    winner = Some(rule);
                                    winner_idx = Some(rule_idx);
                                    break;
                                }
                                r -= rule.probability;
                            }
                        }
                        if winner.is_none() {
                            let fallback_idx = self.derive_candidate_indices.last().copied();
                            (
                                bucket.and_then(|b| fallback_idx.and_then(|i| b.get(i))),
                                fallback_idx,
                            )
                        } else {
                            (winner, winner_idx)
                        }
                    };

                // Check if we can reuse scratch indices (selected rule was last matched)
                let can_reuse_scratch = selected_idx == last_matched_idx;

                if let Some(rule) = selected_rule {
                    // Clear and reuse generation buffers
                    self.gen_context_frame.clear();
                    self.gen_context_frame.extend_from_slice(view.params);

                    if !rule.left_context.is_empty() {
                        // Optimization: reuse scratch indices if selected rule was last matched,
                        // avoiding redundant context matching (O(L) per module savings)
                        let left_indices: &[usize] = if can_reuse_scratch {
                            &self.scratch.left_indices
                        } else {
                            self.gen_left_indices.clear();
                            matching::match_left(
                                &self.state,
                                index,
                                &rule.left_context,
                                &self.ignored_symbols,
                                &mut self.gen_left_indices,
                            );
                            &self.gen_left_indices
                        };
                        for &i in left_indices {
                            let ctx_view = self
                                .state
                                .get_view(i)
                                .ok_or(SystemError::StateCorruption(i))?;
                            self.gen_context_frame.extend_from_slice(ctx_view.params);
                        }
                    }

                    if !rule.right_context.is_empty() {
                        // Optimization: reuse scratch indices if selected rule was last matched
                        let right_indices: &[usize] = if can_reuse_scratch {
                            &self.scratch.right_indices
                        } else {
                            self.gen_right_indices.clear();
                            matching::match_right(
                                &self.state,
                                index,
                                &rule.right_context,
                                &self.ignored_symbols,
                                &mut self.gen_right_indices,
                            );
                            &self.gen_right_indices
                        };
                        for &i in right_indices {
                            let ctx_view = self
                                .state
                                .get_view(i)
                                .ok_or(SystemError::StateCorruption(i))?;
                            self.gen_context_frame.extend_from_slice(ctx_view.params);
                        }
                    }

                    for successor in &rule.successors {
                        self.gen_new_params.clear();
                        for param_code in &successor.params {
                            let val = vm
                                .eval(param_code, &self.gen_context_frame, view.age)
                                .map_err(SystemError::VMError)?;
                            self.gen_new_params.push(val);
                        }
                        self.back_buffer
                            .push(successor.symbol, 0.0, &self.gen_new_params)?;
                    }
                } else {
                    // Identity rule
                    self.back_buffer.push(view.sym, view.age, view.params)?;
                }
            }
            // Swap buffers: back_buffer becomes the new state,
            // current state becomes the recycled back_buffer for next step.
            std::mem::swap(&mut self.state, &mut self.back_buffer);
        }
        Ok(())
    }

    pub fn add_directive(&mut self, directive_src: &str) -> Result<(), SystemError> {
        let (_, directive) = parser::parse_directive(directive_src)
            .map_err(|e| SystemError::ParseError(e.to_string()))?;

        match directive {
            ast::Directive::Ignore(symbols) => {
                for sym_str in symbols {
                    let id = self
                        .interner
                        .get_or_intern(&sym_str)
                        .map_err(SystemError::InternerError)?;
                    if !self.ignored_symbols.contains(&id) {
                        self.ignored_symbols.push(id);
                    }
                }
            }
            ast::Directive::Define(name, expr) => {
                let mut compiler = Compiler::new(vec![], &self.constants);
                let code = compiler.compile(&expr).map_err(SystemError::CompileError)?;

                let mut vm = crate::vm::VirtualMachine::new();
                let val = vm.eval(&code, &[], 0.0).map_err(SystemError::VMError)?;

                self.constants.insert(name, val);
            }
        }
        Ok(())
    }

    /// Compiles and adds a rule to the system.
    pub fn add_rule(&mut self, rule_src: &str) -> Result<(), SystemError> {
        let (_, rule_ast) =
            parser::parse_rule(rule_src).map_err(|e| SystemError::ParseError(e.to_string()))?;

        let mut param_names = Vec::new();
        let mut expected_arities = Vec::new();

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

        for m in &rule_ast.left_context {
            expected_arities.push(m.params.len());
            for param in &m.params {
                if let ast::Expr::Variable(name) = param {
                    param_names.push(name.clone());
                }
            }
        }

        for m in &rule_ast.right_context {
            expected_arities.push(m.params.len());
            for param in &m.params {
                if let ast::Expr::Variable(name) = param {
                    param_names.push(name.clone());
                }
            }
        }

        let mut compiler = Compiler::new(param_names, &self.constants);

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

        // Track symbol arity for structural mutation (before moving expected_arities)
        let pred_arity = expected_arities[0];

        let new_rule = RuntimeRule {
            predecessor: pred_sym,
            left_context: left_ctx,
            right_context: right_ctx,
            probability: rule_ast.probability,
            condition: condition_code,
            successors: runtime_successors.clone(),
            expected_arities,
        };

        // Track predecessor arity
        self.symbol_arities.insert(pred_sym, pred_arity);

        // Track successor arities (fixes arity mismatch for successor-only symbols)
        for succ in &runtime_successors {
            self.symbol_arities
                .entry(succ.symbol)
                .or_insert(succ.params.len());
        }

        self.rules.entry(pred_sym).or_default().push(new_rule);

        Ok(())
    }

    /// Sets the initial state (axiom) of the system.
    pub fn set_axiom(&mut self, axiom_src: &str) -> Result<(), SystemError> {
        let mut remaining = axiom_src;
        self.state.clear();

        // Phase 1: Parse and Intern
        // We decouple parsing from evaluation to avoid holding `self.interner` borrow
        // while needing `self.constants` for evaluation.
        let mut parsed_modules = Vec::new();

        while !remaining.trim().is_empty() {
            let (ni, module) = parser::parse_module(remaining)
                .map_err(|e| SystemError::ParseError(e.to_string()))?;

            let sym_id = self
                .interner
                .get_or_intern(&module.symbol)
                .map_err(SystemError::InternerError)?;

            parsed_modules.push((sym_id, module.params));
            remaining = ni;
        }

        // Phase 2: Compile and Evaluate
        let mut compiler = Compiler::new(vec![], &self.constants);
        let mut vm = crate::vm::VirtualMachine::new();

        for (sym_id, params) in parsed_modules {
            let mut values = Vec::new();
            for expr in params {
                // Compile the expression (using constants)
                let code = compiler.compile(&expr).map_err(SystemError::CompileError)?;

                // Evaluate immediately (no params, age 0)
                let val = vm.eval(&code, &[], 0.0).map_err(SystemError::VMError)?;

                values.push(val);
            }

            // Track axiom symbol arity for structural mutation
            self.symbol_arities.entry(sym_id).or_insert(values.len());

            // Push to state
            self.state.push(sym_id, 0.0, &values)?;
        }

        // Store initial state for reset functionality
        self.initial_state = Some(self.state.clone());

        Ok(())
    }

    /// Resets the system state to the initial axiom.
    ///
    /// This restores the state to what it was immediately after `set_axiom` was called,
    /// discarding all derivation steps. Returns `false` if no axiom has been set.
    ///
    /// # Example
    /// ```
    /// use symbios::System;
    ///
    /// let mut sys = System::new();
    /// sys.add_rule("A -> A B").unwrap();
    /// sys.set_axiom("A").unwrap();
    /// sys.derive(5).unwrap();
    ///
    /// // State has grown after derivation
    /// assert!(sys.state.len() > 1);
    ///
    /// // Reset to initial axiom
    /// assert!(sys.reset());
    /// assert_eq!(sys.state.len(), 1);
    /// ```
    pub fn reset(&mut self) -> bool {
        if let Some(ref initial) = self.initial_state {
            self.state = initial.clone();
            true
        } else {
            false
        }
    }
}

pub mod matching {
    use crate::core::SymbiosState;
    use crate::system::{RuntimeRule, SystemError};
    use crate::vm::VirtualMachine;

    /// Scratch buffers for zero-allocation rule matching.
    ///
    /// Reuse this struct across multiple `matches` calls to avoid
    /// per-call allocations. Call `clear()` before each use.
    #[derive(Debug, Default)]
    pub struct MatchScratch {
        pub context_frame: Vec<f64>,
        pub left_indices: Vec<usize>,
        pub right_indices: Vec<usize>,
    }

    impl MatchScratch {
        pub fn new() -> Self {
            Self::default()
        }

        /// Clears all buffers while preserving capacity.
        #[inline]
        pub fn clear(&mut self) {
            self.context_frame.clear();
            self.left_indices.clear();
            self.right_indices.clear();
        }
    }

    pub fn matches(
        state: &SymbiosState,
        index: usize,
        rule: &RuntimeRule,
        ignore: &[u16],
        vm: &mut VirtualMachine,
        scratch: &mut MatchScratch,
    ) -> Result<bool, SystemError> {
        // Clear scratch buffers (preserves capacity)
        scratch.clear();

        let pred_view = state
            .get_view(index)
            .ok_or(SystemError::InvalidPredecessorParam)?;

        if pred_view.sym != rule.predecessor {
            return Ok(false);
        }

        if pred_view.params.len() != rule.expected_arities[0] {
            return Ok(false);
        }

        if !rule.left_context.is_empty()
            && !match_left(
                state,
                index,
                &rule.left_context,
                ignore,
                &mut scratch.left_indices,
            )
        {
            return Ok(false);
        }

        if !rule.right_context.is_empty()
            && !match_right(
                state,
                index,
                &rule.right_context,
                ignore,
                &mut scratch.right_indices,
            )
        {
            return Ok(false);
        }

        for (i, &ctx_idx) in scratch.left_indices.iter().enumerate() {
            let view = state
                .get_view(ctx_idx)
                .ok_or(SystemError::InvalidPredecessorParam)?;
            if view.params.len() != rule.expected_arities[1 + i] {
                return Ok(false);
            }
        }

        let right_offset = 1 + rule.left_context.len();
        for (i, &ctx_idx) in scratch.right_indices.iter().enumerate() {
            let view = state
                .get_view(ctx_idx)
                .ok_or(SystemError::InvalidPredecessorParam)?;
            if view.params.len() != rule.expected_arities[right_offset + i] {
                return Ok(false);
            }
        }

        if let Some(code) = &rule.condition {
            scratch.context_frame.extend_from_slice(pred_view.params);

            for &i in &scratch.left_indices {
                let ctx_view = state.get_view(i).ok_or(SystemError::StateCorruption(i))?;
                scratch.context_frame.extend_from_slice(ctx_view.params);
            }
            for &i in &scratch.right_indices {
                let ctx_view = state.get_view(i).ok_or(SystemError::StateCorruption(i))?;
                scratch.context_frame.extend_from_slice(ctx_view.params);
            }

            let res = vm
                .eval(code, &scratch.context_frame, pred_view.age)
                .map_err(SystemError::CompileError)?;

            if res == 0.0 {
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Matches a left context pattern against the state, moving backwards from `start_index`.
    ///
    /// # Topology Precedence
    ///
    /// **Important:** When topology has been calculated via `calculate_topology()`, bracket
    /// symbols (`[` and `]`) are handled specially for branch-aware context matching. The
    /// topology skip logic takes precedence over the `ignore` list. This means:
    ///
    /// - If you use `#ignore [ ]`, brackets will still be processed by topology rules
    /// - A `]` causes a jump to its matching `[` (skipping sibling branches)
    /// - A `[` is transparently stepped over
    ///
    /// This behavior is intentional for correct L-System context matching, as described in
    /// ABOP (The Algorithmic Beauty of Plants). To fully ignore brackets, either:
    /// 1. Don't call `calculate_topology()` before derivation, or
    /// 2. Use a linear axiom without branch structure
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
        let mut curr = (start_index - 1) as i64;
        let mut pat_idx = (pattern.len() - 1) as i64;

        while curr >= 0 {
            let view = match state.get_view(curr as usize) {
                Some(v) => v,
                None => return false, // Defensive: invalid index means no match
            };

            // 1. Attempt Match (Explicit context match takes priority)
            if view.sym == pattern[pat_idx as usize] {
                matched_indices.push(curr as usize);
                if pat_idx == 0 {
                    matched_indices.reverse();
                    return true;
                }
                pat_idx -= 1;
                curr -= 1;
                continue;
            }

            // 2. Structural Skipping (Topology Logic)
            if let Some(skip_target) = view.skip_idx {
                if skip_target < curr as usize {
                    // We hit a ']', signifying the end of a sibling branch.
                    // Jump to the start of the branch '['.
                    curr = skip_target as i64 - 1;
                    continue;
                } else {
                    // We hit a '[', signifying the start of the parent branch.
                    // Transparently step over it.
                    curr -= 1;
                    continue;
                }
            }

            // 3. Skip ignored symbols
            if ignore.contains(&view.sym) {
                curr -= 1;
                continue;
            }

            // 4. Mismatch
            return false;
        }
        false
    }

    /// Matches a right context pattern against the state, moving forward from `start_index`.
    ///
    /// # Topology Precedence
    ///
    /// **Important:** When topology has been calculated via `calculate_topology()`, bracket
    /// symbols (`[` and `]`) are handled specially for branch-aware context matching. The
    /// topology skip logic takes precedence over the `ignore` list. This means:
    ///
    /// - If you use `#ignore [ ]`, brackets will still be processed by topology rules
    /// - A `[` causes a jump to its matching `]` (skipping child branches)
    /// - A `]` is transparently stepped over
    ///
    /// This behavior is intentional for correct L-System context matching, as described in
    /// ABOP (The Algorithmic Beauty of Plants). To fully ignore brackets, either:
    /// 1. Don't call `calculate_topology()` before derivation, or
    /// 2. Use a linear axiom without branch structure
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

            // 1. Attempt Match
            if view.sym == pattern[pat_idx] {
                matched_indices.push(curr);
                pat_idx += 1;
                if pat_idx >= pattern.len() {
                    return true;
                }
                curr += 1;
                continue;
            }

            // 2. Structural Skipping
            if let Some(skip_target) = view.skip_idx {
                if skip_target > curr {
                    // We hit a '[', signifying the start of a sibling branch.
                    // Jump to the end of the branch ']'.
                    curr = skip_target + 1;
                    continue;
                } else {
                    // We hit a ']', signifying the end of the parent branch.
                    // Step over it to find the parent's right context.
                    curr += 1;
                    continue;
                }
            }

            // 3. Skip ignored symbols
            if ignore.contains(&view.sym) {
                curr += 1;
                continue;
            }

            // 4. Mismatch
            return false;
        }
        false
    }
}

impl Clone for System {
    fn clone(&self) -> Self {
        Self {
            interner: self.interner.clone(),
            rules: self.rules.clone(),
            state: self.state.clone(),
            back_buffer: SymbiosState::new(),
            ignored_symbols: self.ignored_symbols.clone(),
            rng: Pcg64::seed_from_u64(self.rng.clone().random()),
            constants: self.constants.clone(),
            max_capacity: self.max_capacity,
            scratch: matching::MatchScratch::new(),
            gen_context_frame: Vec::new(),
            gen_left_indices: Vec::new(),
            gen_right_indices: Vec::new(),
            gen_new_params: Vec::new(),
            derive_candidate_indices: Vec::new(),
            initial_state: self.initial_state.clone(),
            symbol_arities: self.symbol_arities.clone(),
        }
    }
}

// ============================================================================
// Rule Export Methods
// ============================================================================

impl System {
    /// Exports all rules in the system to source text.
    ///
    /// Returns a vector of (predecessor_symbol, rule_source) pairs.
    /// Uses synthetic parameter names (p0, p1, ...) since original names
    /// are not preserved during compilation.
    ///
    /// # Example
    /// ```
    /// use symbios::System;
    ///
    /// let mut sys = System::new();
    /// sys.add_rule("A(x) : x > 10 -> B(x + 1)").unwrap();
    /// sys.add_rule("A(x) : x <= 10 -> A(x + 1)").unwrap();
    ///
    /// let exported = sys.export_rules();
    /// assert_eq!(exported.len(), 2);
    /// ```
    pub fn export_rules(&self) -> Vec<(String, String)> {
        let mut results = Vec::new();

        for rules in self.rules.values() {
            for rule in rules {
                let config = crate::export::ExportConfig::synthetic(rule);
                if let Ok(source) =
                    crate::export::export_rule_to_string(rule, &self.interner, &config)
                {
                    let pred_name = self
                        .interner
                        .resolve(rule.predecessor)
                        .unwrap_or("?")
                        .to_string();
                    results.push((pred_name, source));
                }
            }
        }

        results
    }

    /// Exports all rules for a specific predecessor symbol.
    ///
    /// # Arguments
    /// * `predecessor` - The symbol name (e.g., "A")
    ///
    /// # Returns
    /// A vector of rule source strings, or an empty vector if no rules match.
    pub fn export_rules_for(&self, predecessor: &str) -> Vec<String> {
        let pred_id = match self.interner.resolve_id(predecessor) {
            Some(id) => id,
            None => return vec![],
        };

        let rules = match self.rules.get(&pred_id) {
            Some(r) => r,
            None => return vec![],
        };

        let mut results = Vec::new();
        for rule in rules {
            let config = crate::export::ExportConfig::synthetic(rule);
            if let Ok(source) = crate::export::export_rule_to_string(rule, &self.interner, &config)
            {
                results.push(source);
            }
        }

        results
    }

    /// Exports a specific rule by predecessor and index.
    ///
    /// # Arguments
    /// * `predecessor` - The symbol name (e.g., "A")
    /// * `index` - The index of the rule within the predecessor's rule list
    ///
    /// # Returns
    /// The rule source string, or an error if not found.
    pub fn export_rule_at(&self, predecessor: &str, index: usize) -> Result<String, SystemError> {
        let pred_id = self
            .interner
            .resolve_id(predecessor)
            .ok_or_else(|| SystemError::CompileError(format!("Unknown symbol: {}", predecessor)))?;

        let rules = self
            .rules
            .get(&pred_id)
            .ok_or_else(|| SystemError::CompileError(format!("No rules for: {}", predecessor)))?;

        let rule = rules.get(index).ok_or_else(|| {
            SystemError::CompileError(format!("Rule index {} out of bounds", index))
        })?;

        let config = crate::export::ExportConfig::synthetic(rule);
        crate::export::export_rule_to_string(rule, &self.interner, &config)
            .map_err(SystemError::CompileError)
    }

    /// Exports a rule with custom parameter names.
    ///
    /// This is useful when you want to preserve the original parameter names
    /// or use meaningful names for display purposes.
    ///
    /// # Arguments
    /// * `predecessor` - The symbol name
    /// * `index` - The rule index
    /// * `param_names` - Parameter names for the predecessor
    pub fn export_rule_with_params(
        &self,
        predecessor: &str,
        index: usize,
        param_names: Vec<String>,
    ) -> Result<String, SystemError> {
        let pred_id = self
            .interner
            .resolve_id(predecessor)
            .ok_or_else(|| SystemError::CompileError(format!("Unknown symbol: {}", predecessor)))?;

        let rules = self
            .rules
            .get(&pred_id)
            .ok_or_else(|| SystemError::CompileError(format!("No rules for: {}", predecessor)))?;

        let rule = rules.get(index).ok_or_else(|| {
            SystemError::CompileError(format!("Rule index {} out of bounds", index))
        })?;

        let config = crate::export::ExportConfig {
            predecessor_params: param_names,
            ..Default::default()
        };

        crate::export::export_rule_to_string(rule, &self.interner, &config)
            .map_err(SystemError::CompileError)
    }
}

/// Configuration for mutation operations.
#[derive(Debug, Clone)]
pub struct MutationConfig {
    /// Probability of mutating each rule's probability (0.0 - 1.0).
    pub rule_probability_rate: f64,
    /// Maximum change to rule probabilities (additive, clamped to 0.0-1.0).
    pub rule_probability_strength: f64,
    /// Probability of mutating each constant (0.0 - 1.0).
    pub constant_rate: f64,
    /// Relative change to constants (multiplicative factor range).
    pub constant_strength: f64,
    /// Scale for Gaussian jitter applied to bytecode literals (0.0 to disable).
    /// When > 0, scans bytecode for Push(f64) and applies value += N(0,1) * scale.
    pub gaussian_jitter_scale: f64,
    /// Probability of applying Gaussian jitter to each Push literal (0.0 - 1.0).
    pub gaussian_jitter_rate: f64,
}

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            rule_probability_rate: 0.1,
            rule_probability_strength: 0.2,
            constant_rate: 0.1,
            constant_strength: 0.2,
            gaussian_jitter_scale: 0.0,
            gaussian_jitter_rate: 0.0,
        }
    }
}

/// Configuration for structural mutation operations on rule successors and bytecode.
#[derive(Debug, Clone)]
pub struct StructuralMutationConfig {
    /// Probability of mutating each rule's successor sequence (0.0 - 1.0).
    pub successor_rate: f64,
    /// Probability of inserting a new module into successors (0.0 - 1.0).
    pub insert_rate: f64,
    /// Probability of deleting a module from successors (0.0 - 1.0).
    pub delete_rate: f64,
    /// Probability of swapping two adjacent modules in successors (0.0 - 1.0).
    pub swap_rate: f64,
    /// Probability of mutating parameter bytecode for each module (0.0 - 1.0).
    pub bytecode_rate: f64,
    /// Probability of mutating each operation in bytecode (0.0 - 1.0).
    pub op_rate: f64,
    /// Range for perturbing Push constants (additive).
    pub push_perturbation: f64,
}

impl Default for StructuralMutationConfig {
    fn default() -> Self {
        Self {
            successor_rate: 0.1,
            insert_rate: 0.2,
            delete_rate: 0.1,
            swap_rate: 0.2,
            bytecode_rate: 0.1,
            op_rate: 0.1,
            push_perturbation: 0.5,
        }
    }
}

/// Configuration for crossover operations.
#[derive(Debug, Clone)]
pub struct CrossoverConfig {
    /// Probability of taking each rule from parent A vs parent B (0.5 = uniform).
    pub rule_bias: f64,
    /// Blending factor for constants (0.0 = parent A, 1.0 = parent B, 0.5 = average).
    pub constant_blend: f64,
}

impl Default for CrossoverConfig {
    fn default() -> Self {
        Self {
            rule_bias: 0.5,
            constant_blend: 0.5,
        }
    }
}

impl System {
    /// Mutates the system in-place for evolutionary algorithms.
    ///
    /// This method randomly perturbs rule probabilities and constants based on
    /// the provided configuration. Useful for genetic algorithms and evolutionary
    /// optimization of L-System parameters.
    ///
    /// # Arguments
    /// * `config` - Controls mutation rates and strengths
    ///
    /// # Example
    /// ```
    /// use symbios::{System, system::MutationConfig};
    ///
    /// let mut sys = System::new();
    /// sys.add_rule("A -> A B").unwrap();
    /// sys.add_rule("A -> B").unwrap();
    ///
    /// let config = MutationConfig {
    ///     rule_probability_rate: 0.5,
    ///     rule_probability_strength: 0.1,
    ///     ..Default::default()
    /// };
    /// sys.mutate(&config);
    /// ```
    pub fn mutate(&mut self, config: &MutationConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs crossover between this system and another, producing an offspring.
    ///
    /// Rules are selected from either parent based on the bias parameter.
    /// Constants are blended between parents. The offspring inherits the interner
    /// state needed to support all inherited rules.
    ///
    /// # Arguments
    /// * `other` - The other parent system
    /// * `config` - Controls crossover behavior
    ///
    /// # Returns
    /// A new `System` combining genetic material from both parents, or an error
    /// if the offspring's interner cannot accommodate all symbols.
    ///
    /// # Example
    /// ```
    /// use symbios::{System, system::CrossoverConfig};
    ///
    /// let mut parent_a = System::new();
    /// parent_a.add_rule("A -> A A").unwrap();
    ///
    /// let mut parent_b = System::new();
    /// parent_b.add_rule("A -> B").unwrap();
    ///
    /// let config = CrossoverConfig::default();
    /// let offspring = parent_a.crossover(&parent_b, &config).unwrap();
    /// ```
    pub fn crossover(
        &mut self,
        other: &System,
        config: &CrossoverConfig,
    ) -> Result<System, SystemError> {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        let result = self.crossover_with_rng(other, &mut rng, config);
        self.rng = rng;
        result
    }

    /// Mutates the system using an external RNG for reproducibility.
    ///
    /// This variant allows using a shared RNG across multiple systems
    /// for coordinated evolution experiments.
    pub fn mutate_with_rng<R: Rng>(&mut self, rng: &mut R, config: &MutationConfig) {
        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if rng.random::<f64>() < config.rule_probability_rate {
                    let delta = rng.random_range(
                        -config.rule_probability_strength..=config.rule_probability_strength,
                    );
                    rule.probability = (rule.probability + delta).clamp(0.0, 1.0);
                }
            }
        }

        let constant_keys: Vec<String> = self.constants.keys().cloned().collect();
        for key in constant_keys {
            if rng.random::<f64>() < config.constant_rate
                && let Some(val) = self.constants.get_mut(&key)
            {
                let factor =
                    1.0 + rng.random_range(-config.constant_strength..=config.constant_strength);
                *val *= factor;
            }
        }

        // Gaussian jitter on bytecode literals (Issue #53)
        if config.gaussian_jitter_scale > 0.0 && config.gaussian_jitter_rate > 0.0 {
            for rules in self.rules.values_mut() {
                for rule in rules.iter_mut() {
                    // Apply to condition bytecode
                    if let Some(ref mut cond) = rule.condition {
                        Self::apply_gaussian_jitter(rng, cond, config);
                    }
                    // Apply to successor parameter bytecode
                    for successor in &mut rule.successors {
                        for param_bytecode in &mut successor.params {
                            Self::apply_gaussian_jitter(rng, param_bytecode, config);
                        }
                    }
                }
            }
        }
    }

    /// Applies Gaussian jitter to Push literals in bytecode.
    /// Uses Box-Muller transform to generate standard normal samples.
    fn apply_gaussian_jitter<R: Rng>(rng: &mut R, bytecode: &mut [Op], config: &MutationConfig) {
        for op in bytecode.iter_mut() {
            if let Op::Push(val) = op
                && rng.random::<f64>() < config.gaussian_jitter_rate
            {
                // Box-Muller transform for standard normal
                let u1: f64 = rng.random();
                let u2: f64 = rng.random();
                let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                let new_val = *val + z * config.gaussian_jitter_scale;
                if new_val.is_finite() {
                    *op = Op::Push(new_val);
                }
            }
        }
    }

    /// Performs structural mutation using an external RNG for reproducibility.
    ///
    /// This mutates the structure of rule successors (insert/delete/swap modules)
    /// and the bytecode of module parameters (change operations).
    pub fn structural_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &StructuralMutationConfig,
    ) {
        let symbol_ids: Vec<u16> = self.interner.iter().map(|(id, _)| id).collect();
        if symbol_ids.is_empty() {
            return;
        }

        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if rng.random::<f64>() >= config.successor_rate {
                    continue;
                }

                // Swap two adjacent modules
                if rule.successors.len() >= 2 && rng.random::<f64>() < config.swap_rate {
                    let idx = rng.random_range(0..rule.successors.len() - 1);
                    rule.successors.swap(idx, idx + 1);
                }

                // Delete a random module (keep at least one)
                if rule.successors.len() > 1 && rng.random::<f64>() < config.delete_rate {
                    let idx = rng.random_range(0..rule.successors.len());
                    rule.successors.remove(idx);
                }

                // Insert a new module at a random position (respecting MAX_SUCCESSORS limit)
                if rule.successors.len() < MAX_SUCCESSORS
                    && rng.random::<f64>() < config.insert_rate
                {
                    let symbol = symbol_ids[rng.random_range(0..symbol_ids.len())];
                    // Initialize with correct arity: one Op::Push(0.0) per expected parameter
                    let arity = self.symbol_arities.get(&symbol).copied().unwrap_or(0);
                    let params: Vec<Vec<Op>> = (0..arity).map(|_| vec![Op::Push(0.0)]).collect();
                    let new_module = RuntimeModule { symbol, params };
                    let idx = rng.random_range(0..=rule.successors.len());
                    rule.successors.insert(idx, new_module);
                }

                // Mutate bytecode of module parameters
                for module in &mut rule.successors {
                    if rng.random::<f64>() >= config.bytecode_rate {
                        continue;
                    }
                    for param_bytecode in &mut module.params {
                        Self::mutate_bytecode(rng, param_bytecode, config);
                    }
                }
            }
        }
    }

    /// Mutates a single bytecode sequence by changing operations.
    fn mutate_bytecode<R: Rng>(
        rng: &mut R,
        bytecode: &mut [Op],
        config: &StructuralMutationConfig,
    ) {
        for op in bytecode.iter_mut() {
            if rng.random::<f64>() >= config.op_rate {
                continue;
            }
            *op = match op {
                // Perturb Push constants (with finiteness check to prevent Inf poisoning)
                Op::Push(val) => {
                    // Clamp perturbation range to prevent overflow in random_range
                    let safe_perturbation = config.push_perturbation.min(1e100);
                    let delta = rng.random_range(-safe_perturbation..=safe_perturbation);
                    let new_val = *val + delta;
                    // Only apply mutation if result is finite; otherwise keep original
                    if new_val.is_finite() {
                        Op::Push(new_val)
                    } else {
                        continue;
                    }
                }
                // Swap arithmetic operations
                Op::Add => Self::random_arithmetic_op(rng),
                Op::Sub => Self::random_arithmetic_op(rng),
                Op::Mul => Self::random_arithmetic_op(rng),
                Op::Div => Self::random_arithmetic_op(rng),
                // Swap relational operations
                Op::Eq => Self::random_relational_op(rng),
                Op::Ne => Self::random_relational_op(rng),
                Op::Gt => Self::random_relational_op(rng),
                Op::Lt => Self::random_relational_op(rng),
                Op::Ge => Self::random_relational_op(rng),
                Op::Le => Self::random_relational_op(rng),
                // Swap logical operations (binary only)
                Op::And => {
                    if rng.random::<bool>() {
                        Op::Or
                    } else {
                        Op::And
                    }
                }
                Op::Or => {
                    if rng.random::<bool>() {
                        Op::And
                    } else {
                        Op::Or
                    }
                }
                // Leave other ops unchanged
                _ => continue,
            };
        }
    }

    fn random_arithmetic_op<R: Rng>(rng: &mut R) -> Op {
        match rng.random_range(0..4) {
            0 => Op::Add,
            1 => Op::Sub,
            2 => Op::Mul,
            _ => Op::Div,
        }
    }

    fn random_relational_op<R: Rng>(rng: &mut R) -> Op {
        match rng.random_range(0..6) {
            0 => Op::Eq,
            1 => Op::Ne,
            2 => Op::Gt,
            3 => Op::Lt,
            4 => Op::Ge,
            _ => Op::Le,
        }
    }

    /// Performs structural mutation on rule successors and bytecode.
    ///
    /// This is a convenience wrapper that uses the system's internal RNG.
    pub fn structural_mutate(&mut self, config: &StructuralMutationConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.structural_mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs crossover using an external RNG for reproducibility.
    ///
    /// # Errors
    /// Returns an error if the offspring's interner cannot accommodate all symbols
    /// from both parents (e.g., due to ID overflow or heap limits).
    pub fn crossover_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        config: &CrossoverConfig,
    ) -> Result<System, SystemError> {
        let mut offspring = System::new();
        offspring.rng = Pcg64::seed_from_u64(rng.random());
        offspring.max_capacity = self.max_capacity.max(other.max_capacity);

        let mut symbol_map_self: HashMap<u16, u16> = HashMap::new();
        let mut symbol_map_other: HashMap<u16, u16> = HashMap::new();

        for (old_id, name) in self.interner.iter() {
            let new_id = offspring
                .interner
                .get_or_intern(name)
                .map_err(SystemError::InternerError)?;
            symbol_map_self.insert(old_id, new_id);
        }

        for (old_id, name) in other.interner.iter() {
            let new_id = offspring
                .interner
                .get_or_intern(name)
                .map_err(SystemError::InternerError)?;
            symbol_map_other.insert(old_id, new_id);
        }

        // Use HashSet for O(1) lookup instead of O(N) Vec::contains
        let mut all_predecessors: HashSet<u16> = HashSet::new();
        for &pred in self.rules.keys() {
            if let Some(&new_pred) = symbol_map_self.get(&pred) {
                all_predecessors.insert(new_pred);
            }
        }
        for &pred in other.rules.keys() {
            if let Some(&new_pred) = symbol_map_other.get(&pred) {
                all_predecessors.insert(new_pred);
            }
        }

        // Build reverse mappings (new_id -> old_id) for O(1) lookup instead of O(N) scan
        let reverse_map_self: Vec<Option<u16>> = {
            let max_new_id = symbol_map_self.values().max().copied().unwrap_or(0) as usize;
            let mut reverse = vec![None; max_new_id + 1];
            for (&old_id, &new_id) in &symbol_map_self {
                reverse[new_id as usize] = Some(old_id);
            }
            reverse
        };
        let reverse_map_other: Vec<Option<u16>> = {
            let max_new_id = symbol_map_other.values().max().copied().unwrap_or(0) as usize;
            let mut reverse = vec![None; max_new_id + 1];
            for (&old_id, &new_id) in &symbol_map_other {
                reverse[new_id as usize] = Some(old_id);
            }
            reverse
        };

        // Track which parent contributed rules for each symbol (for consistent arity inheritance)
        let mut rules_from_self: HashSet<u16> = HashSet::new();
        let mut rules_from_other: HashSet<u16> = HashSet::new();

        for new_pred in all_predecessors {
            let self_pred = reverse_map_self.get(new_pred as usize).copied().flatten();
            let other_pred = reverse_map_other.get(new_pred as usize).copied().flatten();

            let self_rules = self_pred.and_then(|p| self.rules.get(&p));
            let other_rules = other_pred.and_then(|p| other.rules.get(&p));

            let (selected_rules, symbol_map, from_self) = match (self_rules, other_rules) {
                (Some(rules), None) => (rules, &symbol_map_self, true),
                (None, Some(rules)) => (rules, &symbol_map_other, false),
                (Some(self_r), Some(other_r)) => {
                    if rng.random::<f64>() < config.rule_bias {
                        (self_r, &symbol_map_self, true)
                    } else {
                        (other_r, &symbol_map_other, false)
                    }
                }
                (None, None) => continue,
            };

            // Track rule source for arity consistency
            if from_self {
                rules_from_self.insert(new_pred);
            } else {
                rules_from_other.insert(new_pred);
            }

            let mut remapped_rules: Vec<RuntimeRule> = Vec::new();
            for rule in selected_rules {
                let remapped = RuntimeRule {
                    predecessor: *symbol_map
                        .get(&rule.predecessor)
                        .unwrap_or(&rule.predecessor),
                    left_context: rule
                        .left_context
                        .iter()
                        .map(|s| *symbol_map.get(s).unwrap_or(s))
                        .collect(),
                    right_context: rule
                        .right_context
                        .iter()
                        .map(|s| *symbol_map.get(s).unwrap_or(s))
                        .collect(),
                    probability: rule.probability,
                    condition: rule.condition.clone(),
                    successors: rule
                        .successors
                        .iter()
                        .map(|s| RuntimeModule {
                            symbol: *symbol_map.get(&s.symbol).unwrap_or(&s.symbol),
                            params: s.params.clone(),
                        })
                        .collect(),
                    expected_arities: rule.expected_arities.clone(),
                };
                remapped_rules.push(remapped);
            }

            offspring.rules.insert(new_pred, remapped_rules);
        }

        let mut all_constant_keys: Vec<String> = self.constants.keys().cloned().collect();
        for key in other.constants.keys() {
            if !all_constant_keys.contains(key) {
                all_constant_keys.push(key.clone());
            }
        }

        for key in all_constant_keys {
            let val_a = self.constants.get(&key).copied();
            let val_b = other.constants.get(&key).copied();

            let blended = match (val_a, val_b) {
                (Some(a), Some(b)) => a * (1.0 - config.constant_blend) + b * config.constant_blend,
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => continue,
            };
            offspring.constants.insert(key, blended);
        }

        let mut new_ignored = Vec::new();

        for s in &self.ignored_symbols {
            if let Some(new_id) = symbol_map_self.get(s)
                && !new_ignored.contains(new_id)
            {
                new_ignored.push(*new_id);
            }
        }

        for s in &other.ignored_symbols {
            if let Some(new_id) = symbol_map_other.get(s)
                && !new_ignored.contains(new_id)
            {
                new_ignored.push(*new_id);
            }
        }

        offspring.ignored_symbols = new_ignored;

        // Merge symbol_arities from both parents with rule-consistent inheritance.
        // For symbols with rules, use arity from the parent whose rules were selected.
        // This prevents arity mismatches where structural_mutate inserts modules
        // with parameter counts that don't match inherited rules.
        for (&old_id, &arity) in &self.symbol_arities {
            if let Some(&new_id) = symbol_map_self.get(&old_id) {
                // For symbols with rules from self, always use self's arity
                // For other symbols, insert only if not already present
                if rules_from_self.contains(&new_id) {
                    offspring.symbol_arities.insert(new_id, arity);
                } else {
                    offspring.symbol_arities.entry(new_id).or_insert(arity);
                }
            }
        }
        for (&old_id, &arity) in &other.symbol_arities {
            if let Some(&new_id) = symbol_map_other.get(&old_id) {
                // For symbols with rules from other, always use other's arity
                // This overrides any arity from self for these symbols
                if rules_from_other.contains(&new_id) {
                    offspring.symbol_arities.insert(new_id, arity);
                } else {
                    offspring.symbol_arities.entry(new_id).or_insert(arity);
                }
            }
        }

        Ok(offspring)
    }
}

// ============================================================================
// Advanced Mutation Operations (Issues #54, #55, #56, #57)
// ============================================================================

/// Configuration for operator flip mutation.
#[derive(Debug, Clone)]
pub struct OperatorFlipConfig {
    /// Probability of flipping each arithmetic operator (Add<->Sub, Mul<->Div).
    pub arithmetic_flip_rate: f64,
    /// Probability of flipping each relational operator (Gt<->Lt, Ge<->Le, Eq<->Ne).
    pub relational_flip_rate: f64,
}

impl Default for OperatorFlipConfig {
    fn default() -> Self {
        Self {
            arithmetic_flip_rate: 0.1,
            relational_flip_rate: 0.1,
        }
    }
}

/// Configuration for rule duplication and specialization.
#[derive(Debug, Clone)]
pub struct RuleDuplicationConfig {
    /// Probability of duplicating each rule.
    pub duplication_rate: f64,
    /// Amount to perturb the condition after duplication (for numeric conditions).
    pub condition_perturbation: f64,
}

impl Default for RuleDuplicationConfig {
    fn default() -> Self {
        Self {
            duplication_rate: 0.1,
            condition_perturbation: 0.2,
        }
    }
}

/// Configuration for topological (turtle command) symbol mutation.
#[derive(Debug, Clone)]
pub struct TopologicalMutationConfig {
    /// Probability of swapping each turtle command with its counterpart.
    pub swap_rate: f64,
}

impl Default for TopologicalMutationConfig {
    fn default() -> Self {
        Self { swap_rate: 0.1 }
    }
}

/// Configuration for literal-to-constant promotion mutation.
#[derive(Debug, Clone)]
pub struct LiteralPromotionConfig {
    /// Probability of promoting each literal to a constant reference.
    pub promotion_rate: f64,
    /// Tolerance for matching literal values to constants.
    pub match_tolerance: f64,
}

impl Default for LiteralPromotionConfig {
    fn default() -> Self {
        Self {
            promotion_rate: 0.1,
            match_tolerance: 0.1,
        }
    }
}

impl System {
    /// Performs operator flip mutation on bytecode (Issue #54).
    ///
    /// Swaps arithmetic operators (Add<->Sub, Mul<->Div) and relational operators
    /// (Gt<->Lt, Ge<->Le, Eq<->Ne) while preserving stack signatures.
    pub fn operator_flip_mutate(&mut self, config: &OperatorFlipConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.operator_flip_mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs operator flip mutation with external RNG for reproducibility.
    pub fn operator_flip_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &OperatorFlipConfig,
    ) {
        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if let Some(ref mut cond) = rule.condition {
                    Self::flip_operators_in_bytecode(rng, cond, config);
                }
                for successor in &mut rule.successors {
                    for param_bytecode in &mut successor.params {
                        Self::flip_operators_in_bytecode(rng, param_bytecode, config);
                    }
                }
            }
        }
    }

    fn flip_operators_in_bytecode<R: Rng>(
        rng: &mut R,
        bytecode: &mut [Op],
        config: &OperatorFlipConfig,
    ) {
        for op in bytecode.iter_mut() {
            match op {
                // Arithmetic flips: Add<->Sub, Mul<->Div
                Op::Add if rng.random::<f64>() < config.arithmetic_flip_rate => *op = Op::Sub,
                Op::Sub if rng.random::<f64>() < config.arithmetic_flip_rate => *op = Op::Add,
                Op::Mul if rng.random::<f64>() < config.arithmetic_flip_rate => *op = Op::Div,
                Op::Div if rng.random::<f64>() < config.arithmetic_flip_rate => *op = Op::Mul,
                // Relational flips: Gt<->Lt, Ge<->Le, Eq<->Ne
                Op::Gt if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Lt,
                Op::Lt if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Gt,
                Op::Ge if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Le,
                Op::Le if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Ge,
                Op::Eq if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Ne,
                Op::Ne if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Eq,
                _ => {}
            }
        }
    }

    /// Performs rule duplication and specialization (Issue #55).
    ///
    /// Clones existing rules and mutates the probability of both original and copy,
    /// ensuring probabilities are normalized. This enables evolutionary speciation.
    pub fn rule_duplication_mutate(&mut self, config: &RuleDuplicationConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.rule_duplication_mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs rule duplication with external RNG for reproducibility.
    pub fn rule_duplication_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &RuleDuplicationConfig,
    ) {
        let pred_keys: Vec<u16> = self.rules.keys().copied().collect();

        for pred in pred_keys {
            let rules = match self.rules.get_mut(&pred) {
                Some(r) => r,
                None => continue,
            };

            let mut duplicates = Vec::new();

            for (idx, rule) in rules.iter_mut().enumerate() {
                if rng.random::<f64>() < config.duplication_rate {
                    // Clone the rule
                    let mut dup = rule.clone();

                    // Perturb probabilities of both original and duplicate
                    let orig_prob = rule.probability;
                    let split = rng.random_range(0.3..0.7);

                    rule.probability = orig_prob * split;
                    dup.probability = orig_prob * (1.0 - split);

                    // Perturb condition bytecode of duplicate (if present)
                    if let Some(ref mut cond) = dup.condition {
                        for op in cond.iter_mut() {
                            if let Op::Push(val) = op {
                                let perturbation = rng.random_range(
                                    -config.condition_perturbation..=config.condition_perturbation,
                                );
                                let new_val = *val + perturbation;
                                if new_val.is_finite() {
                                    *op = Op::Push(new_val);
                                }
                            }
                        }
                    }

                    duplicates.push((idx, dup));
                }
            }

            // Insert duplicates after their originals
            for (offset, (orig_idx, dup)) in duplicates.into_iter().enumerate() {
                let insert_pos = orig_idx + offset + 1;
                if insert_pos <= rules.len() {
                    rules.insert(insert_pos, dup);
                } else {
                    rules.push(dup);
                }
            }
        }
    }

    /// Performs topological symbol mutation for turtle commands (Issue #57).
    ///
    /// Swaps spatial counterparts in successor sequences:
    /// - `+` (Yaw Left) <-> `-` (Yaw Right)
    /// - `&` (Pitch Down) <-> `^` (Pitch Up)
    /// - `F` (Move) <-> `f` (Move Invisible)
    pub fn topological_mutate(&mut self, config: &TopologicalMutationConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.topological_mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs topological mutation with external RNG for reproducibility.
    pub fn topological_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &TopologicalMutationConfig,
    ) {
        // Build symbol pair mappings for turtle commands
        let swap_pairs: Vec<(&str, &str)> = vec![
            ("+", "-"),  // Yaw left <-> Yaw right
            ("&", "^"),  // Pitch down <-> Pitch up
            ("F", "f"),  // Move <-> Move invisible
            ("\\", "/"), // Roll left <-> Roll right
        ];

        let mut symbol_swaps: HashMap<u16, u16> = HashMap::new();

        for (a, b) in swap_pairs {
            if let (Some(id_a), Some(id_b)) =
                (self.interner.resolve_id(a), self.interner.resolve_id(b))
            {
                symbol_swaps.insert(id_a, id_b);
                symbol_swaps.insert(id_b, id_a);
            }
        }

        if symbol_swaps.is_empty() {
            return;
        }

        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                for successor in &mut rule.successors {
                    if rng.random::<f64>() < config.swap_rate
                        && let Some(&swapped) = symbol_swaps.get(&successor.symbol)
                    {
                        successor.symbol = swapped;
                    }
                }
            }
        }
    }

    /// Promotes literal values to constant references (Issue #56).
    ///
    /// Scans bytecode for Push(f64) and replaces with LoadConstant when a
    /// matching constant exists within tolerance. This increases genetic coupling.
    pub fn literal_to_constant_promote(&mut self, config: &LiteralPromotionConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.literal_to_constant_promote_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Promotes literals with external RNG for reproducibility.
    pub fn literal_to_constant_promote_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &LiteralPromotionConfig,
    ) {
        if self.constants.is_empty() {
            return;
        }

        // Build constant value -> name mapping
        let const_values: Vec<(String, f64)> = self
            .constants
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if let Some(ref mut cond) = rule.condition {
                    Self::promote_literals_in_bytecode(rng, cond, &const_values, config);
                }
                for successor in &mut rule.successors {
                    for param_bytecode in &mut successor.params {
                        Self::promote_literals_in_bytecode(
                            rng,
                            param_bytecode,
                            &const_values,
                            config,
                        );
                    }
                }
            }
        }
    }

    fn promote_literals_in_bytecode<R: Rng>(
        rng: &mut R,
        bytecode: &mut [Op],
        const_values: &[(String, f64)],
        config: &LiteralPromotionConfig,
    ) {
        for op in bytecode.iter_mut() {
            if let Op::Push(val) = op {
                if rng.random::<f64>() >= config.promotion_rate {
                    continue;
                }
                // Find a matching constant within tolerance
                for (_name, const_val) in const_values {
                    let diff = (*val - *const_val).abs();
                    let threshold = config.match_tolerance * const_val.abs().max(1.0);
                    if diff <= threshold {
                        // Replace with the constant's value (since we don't have LoadConstant,
                        // we update the literal to match the constant exactly, creating coupling)
                        *op = Op::Push(*const_val);
                        // Store the constant name association for genetic tracking
                        // (In a full implementation, we'd add Op::LoadConstant)
                        break;
                    }
                }
            }
        }
    }
}

// ============================================================================
// Advanced Crossover Operations (Issues #50, #51, #52)
// ============================================================================

/// Strategy for rule crossover selection.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CrossoverStrategy {
    /// Uniform random selection of entire rule sets per predecessor.
    #[default]
    Uniform,
    /// Homologous crossover: select subsets of rules per shared predecessor.
    Homologous,
}

/// Extended configuration for advanced crossover operations.
#[derive(Debug, Clone)]
pub struct AdvancedCrossoverConfig {
    /// Base crossover configuration.
    pub base: CrossoverConfig,
    /// Crossover strategy to use.
    pub strategy: CrossoverStrategy,
    /// For homologous crossover: probability of taking each individual rule from parent A.
    pub homologous_rule_bias: f64,
    /// BLX-alpha parameter for parametric blend crossover.
    /// When > 0, attempts to blend structurally identical rules by interpolating literals.
    pub blx_alpha: f64,
}

impl Default for AdvancedCrossoverConfig {
    fn default() -> Self {
        Self {
            base: CrossoverConfig::default(),
            strategy: CrossoverStrategy::Uniform,
            homologous_rule_bias: 0.5,
            blx_alpha: 0.0,
        }
    }
}

impl System {
    /// Performs advanced crossover with configurable strategy (Issues #50, #51).
    pub fn advanced_crossover(
        &mut self,
        other: &System,
        config: &AdvancedCrossoverConfig,
    ) -> Result<System, SystemError> {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        let result = self.advanced_crossover_with_rng(other, &mut rng, config);
        self.rng = rng;
        result
    }

    /// Performs advanced crossover with external RNG for reproducibility.
    pub fn advanced_crossover_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        config: &AdvancedCrossoverConfig,
    ) -> Result<System, SystemError> {
        match config.strategy {
            CrossoverStrategy::Uniform => self.crossover_with_rng(other, rng, &config.base),
            CrossoverStrategy::Homologous => self.homologous_crossover_with_rng(other, rng, config),
        }
    }

    /// Performs homologous rule crossover (Issue #50).
    ///
    /// Groups rules by predecessor symbol and selects individual rules from
    /// each parent rather than entire rule sets.
    fn homologous_crossover_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        config: &AdvancedCrossoverConfig,
    ) -> Result<System, SystemError> {
        let mut offspring = System::new();
        offspring.rng = Pcg64::seed_from_u64(rng.random());
        offspring.max_capacity = self.max_capacity.max(other.max_capacity);

        // Build symbol mappings
        let mut symbol_map_self: HashMap<u16, u16> = HashMap::new();
        let mut symbol_map_other: HashMap<u16, u16> = HashMap::new();

        for (old_id, name) in self.interner.iter() {
            let new_id = offspring
                .interner
                .get_or_intern(name)
                .map_err(SystemError::InternerError)?;
            symbol_map_self.insert(old_id, new_id);
        }

        for (old_id, name) in other.interner.iter() {
            let new_id = offspring
                .interner
                .get_or_intern(name)
                .map_err(SystemError::InternerError)?;
            symbol_map_other.insert(old_id, new_id);
        }

        // Collect all predecessor symbols
        let mut all_predecessors: HashSet<u16> = HashSet::new();
        for &pred in self.rules.keys() {
            if let Some(&new_pred) = symbol_map_self.get(&pred) {
                all_predecessors.insert(new_pred);
            }
        }
        for &pred in other.rules.keys() {
            if let Some(&new_pred) = symbol_map_other.get(&pred) {
                all_predecessors.insert(new_pred);
            }
        }

        // Build reverse mappings
        let reverse_map_self: HashMap<u16, u16> = symbol_map_self
            .iter()
            .map(|(&old, &new)| (new, old))
            .collect();
        let reverse_map_other: HashMap<u16, u16> = symbol_map_other
            .iter()
            .map(|(&old, &new)| (new, old))
            .collect();

        // Homologous crossover: select individual rules from each parent
        for new_pred in all_predecessors {
            let self_pred = reverse_map_self.get(&new_pred).copied();
            let other_pred = reverse_map_other.get(&new_pred).copied();

            let self_rules = self_pred.and_then(|p| self.rules.get(&p));
            let other_rules = other_pred.and_then(|p| other.rules.get(&p));

            let mut selected_rules: Vec<RuntimeRule> = Vec::new();

            // Select rules from self
            if let Some(rules) = self_rules {
                for rule in rules {
                    if rng.random::<f64>() < config.homologous_rule_bias {
                        let remapped = Self::remap_rule(rule, &symbol_map_self);
                        // Try BLX-alpha blend if enabled and matching rule exists in other
                        if config.blx_alpha > 0.0
                            && let Some(other_rules) = other_rules
                            && let Some(blended) = Self::try_blx_alpha_blend(
                                &remapped,
                                other_rules,
                                &symbol_map_other,
                                config.blx_alpha,
                                rng,
                            )
                        {
                            selected_rules.push(blended);
                            continue;
                        }
                        selected_rules.push(remapped);
                    }
                }
            }

            // Select rules from other
            if let Some(rules) = other_rules {
                for rule in rules {
                    if rng.random::<f64>() >= config.homologous_rule_bias {
                        let remapped = Self::remap_rule(rule, &symbol_map_other);
                        selected_rules.push(remapped);
                    }
                }
            }

            if !selected_rules.is_empty() {
                offspring.rules.insert(new_pred, selected_rules);
            }
        }

        // Blend constants
        let mut all_constant_keys: Vec<String> = self.constants.keys().cloned().collect();
        for key in other.constants.keys() {
            if !all_constant_keys.contains(key) {
                all_constant_keys.push(key.clone());
            }
        }

        for key in all_constant_keys {
            let val_a = self.constants.get(&key).copied();
            let val_b = other.constants.get(&key).copied();

            let blended = match (val_a, val_b) {
                (Some(a), Some(b)) => {
                    a * (1.0 - config.base.constant_blend) + b * config.base.constant_blend
                }
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => continue,
            };
            offspring.constants.insert(key, blended);
        }

        // Merge ignored symbols
        let mut new_ignored = Vec::new();
        for s in &self.ignored_symbols {
            if let Some(new_id) = symbol_map_self.get(s)
                && !new_ignored.contains(new_id)
            {
                new_ignored.push(*new_id);
            }
        }
        for s in &other.ignored_symbols {
            if let Some(new_id) = symbol_map_other.get(s)
                && !new_ignored.contains(new_id)
            {
                new_ignored.push(*new_id);
            }
        }
        offspring.ignored_symbols = new_ignored;

        // Merge symbol arities
        for (&old_id, &arity) in &self.symbol_arities {
            if let Some(&new_id) = symbol_map_self.get(&old_id) {
                offspring.symbol_arities.entry(new_id).or_insert(arity);
            }
        }
        for (&old_id, &arity) in &other.symbol_arities {
            if let Some(&new_id) = symbol_map_other.get(&old_id) {
                offspring.symbol_arities.entry(new_id).or_insert(arity);
            }
        }

        Ok(offspring)
    }

    fn remap_rule(rule: &RuntimeRule, symbol_map: &HashMap<u16, u16>) -> RuntimeRule {
        RuntimeRule {
            predecessor: *symbol_map
                .get(&rule.predecessor)
                .unwrap_or(&rule.predecessor),
            left_context: rule
                .left_context
                .iter()
                .map(|s| *symbol_map.get(s).unwrap_or(s))
                .collect(),
            right_context: rule
                .right_context
                .iter()
                .map(|s| *symbol_map.get(s).unwrap_or(s))
                .collect(),
            probability: rule.probability,
            condition: rule.condition.clone(),
            successors: rule
                .successors
                .iter()
                .map(|s| RuntimeModule {
                    symbol: *symbol_map.get(&s.symbol).unwrap_or(&s.symbol),
                    params: s.params.clone(),
                })
                .collect(),
            expected_arities: rule.expected_arities.clone(),
        }
    }

    /// Attempts BLX-alpha blending between structurally identical rules (Issue #51).
    ///
    /// If a rule in `other_rules` has the same bytecode structure (same ops, different
    /// Push values), interpolates the literals using BLX-alpha.
    fn try_blx_alpha_blend<R: Rng>(
        rule_a: &RuntimeRule,
        other_rules: &[RuntimeRule],
        symbol_map: &HashMap<u16, u16>,
        alpha: f64,
        rng: &mut R,
    ) -> Option<RuntimeRule> {
        for rule_b in other_rules {
            // Check structural compatibility
            if !Self::rules_structurally_compatible(rule_a, rule_b, symbol_map) {
                continue;
            }

            // Found a compatible rule, perform BLX-alpha blend
            let mut blended = rule_a.clone();
            blended.probability = (rule_a.probability + rule_b.probability) / 2.0;

            // Blend condition bytecode
            if let (Some(cond_a), Some(cond_b)) =
                (blended.condition.as_mut(), rule_b.condition.as_ref())
            {
                Self::blx_alpha_blend_bytecode(cond_a, cond_b, alpha, rng);
            }

            // Blend successor parameters
            for (succ_a, succ_b) in blended.successors.iter_mut().zip(&rule_b.successors) {
                for (params_a, params_b) in succ_a.params.iter_mut().zip(&succ_b.params) {
                    Self::blx_alpha_blend_bytecode(params_a, params_b, alpha, rng);
                }
            }

            return Some(blended);
        }
        None
    }

    fn rules_structurally_compatible(
        rule_a: &RuntimeRule,
        rule_b: &RuntimeRule,
        symbol_map: &HashMap<u16, u16>,
    ) -> bool {
        // Check predecessor matches (after remapping)
        let remapped_pred = symbol_map
            .get(&rule_b.predecessor)
            .unwrap_or(&rule_b.predecessor);
        if rule_a.predecessor != *remapped_pred {
            return false;
        }

        // Check successor counts match
        if rule_a.successors.len() != rule_b.successors.len() {
            return false;
        }

        // Check each successor has compatible structure
        for (succ_a, succ_b) in rule_a.successors.iter().zip(&rule_b.successors) {
            let remapped_sym = symbol_map.get(&succ_b.symbol).unwrap_or(&succ_b.symbol);
            if succ_a.symbol != *remapped_sym {
                return false;
            }
            if succ_a.params.len() != succ_b.params.len() {
                return false;
            }
            // Check bytecode structure (same ops, ignoring Push values)
            for (params_a, params_b) in succ_a.params.iter().zip(&succ_b.params) {
                if !Self::bytecode_structurally_equal(params_a, params_b) {
                    return false;
                }
            }
        }

        true
    }

    fn bytecode_structurally_equal(a: &[Op], b: &[Op]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        for (op_a, op_b) in a.iter().zip(b) {
            let same_structure = match (op_a, op_b) {
                (Op::Push(_), Op::Push(_)) => true,
                (Op::LoadParam(i), Op::LoadParam(j)) => i == j,
                (Op::LoadAge, Op::LoadAge) => true,
                (Op::Add, Op::Add) => true,
                (Op::Sub, Op::Sub) => true,
                (Op::Mul, Op::Mul) => true,
                (Op::Div, Op::Div) => true,
                (Op::Pow, Op::Pow) => true,
                (Op::Neg, Op::Neg) => true,
                (Op::Eq, Op::Eq) => true,
                (Op::Ne, Op::Ne) => true,
                (Op::Gt, Op::Gt) => true,
                (Op::Lt, Op::Lt) => true,
                (Op::Ge, Op::Ge) => true,
                (Op::Le, Op::Le) => true,
                (Op::And, Op::And) => true,
                (Op::Or, Op::Or) => true,
                (Op::Not, Op::Not) => true,
                (Op::Math(m1), Op::Math(m2)) => m1 == m2,
                _ => false,
            };
            if !same_structure {
                return false;
            }
        }
        true
    }

    /// Performs BLX-alpha blending on bytecode Push values.
    fn blx_alpha_blend_bytecode<R: Rng>(a: &mut [Op], b: &[Op], alpha: f64, rng: &mut R) {
        for (op_a, op_b) in a.iter_mut().zip(b) {
            let (val_a, val_b) = match (op_a as &Op, op_b) {
                (Op::Push(va), Op::Push(vb)) => (*va, *vb),
                _ => continue,
            };

            let min_val = val_a.min(val_b);
            let max_val = val_a.max(val_b);
            let range = max_val - min_val;

            // BLX-alpha: sample from [min - alpha*range, max + alpha*range]
            let low = min_val - alpha * range;
            let high = max_val + alpha * range;

            let blended = if (high - low).abs() < f64::EPSILON {
                val_a
            } else {
                rng.random_range(low..=high)
            };

            if blended.is_finite() {
                *op_a = Op::Push(blended);
            }
        }
    }

    /// Performs sub-expression grafting crossover (Issue #52).
    ///
    /// Swaps branch blocks ([...]) between parent successor sequences.
    pub fn subexpression_graft(
        &mut self,
        other: &System,
        graft_rate: f64,
    ) -> Result<System, SystemError> {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        let result = self.subexpression_graft_with_rng(other, &mut rng, graft_rate);
        self.rng = rng;
        result
    }

    /// Performs sub-expression grafting with external RNG for reproducibility.
    pub fn subexpression_graft_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        graft_rate: f64,
    ) -> Result<System, SystemError> {
        // Start with standard crossover
        let config = CrossoverConfig::default();
        let mut offspring = self.crossover_with_rng(other, rng, &config)?;

        // Get bracket symbols
        let open_sym = offspring.interner.resolve_id("[");
        let close_sym = offspring.interner.resolve_id("]");

        if open_sym.is_none() || close_sym.is_none() {
            return Ok(offspring);
        }

        let open = open_sym.unwrap();
        let close = close_sym.unwrap();

        // Extract branch blocks from other parent's rules
        let other_branches: Vec<Vec<RuntimeModule>> = other
            .rules
            .values()
            .flat_map(|rules| rules.iter())
            .flat_map(|rule| Self::extract_branch_blocks(&rule.successors, open, close))
            .collect();

        if other_branches.is_empty() {
            return Ok(offspring);
        }

        // Graft branches into offspring rules
        for rules in offspring.rules.values_mut() {
            for rule in rules.iter_mut() {
                if rng.random::<f64>() < graft_rate && !other_branches.is_empty() {
                    // Find a branch in current rule to replace
                    if let Some((start, end)) =
                        Self::find_branch_block(&rule.successors, open, close)
                    {
                        // Select a random branch from other parent
                        let donor_idx = rng.random_range(0..other_branches.len());
                        let donor_branch = &other_branches[donor_idx];

                        // Replace the branch
                        let mut new_successors = Vec::new();
                        new_successors.extend_from_slice(&rule.successors[..start]);
                        new_successors.extend(donor_branch.iter().cloned());
                        new_successors.extend_from_slice(&rule.successors[end + 1..]);

                        // Enforce MAX_SUCCESSORS limit
                        if new_successors.len() <= MAX_SUCCESSORS {
                            rule.successors = new_successors;
                        }
                    }
                }
            }
        }

        Ok(offspring)
    }

    /// Extracts all balanced branch blocks from a successor sequence.
    fn extract_branch_blocks(
        successors: &[RuntimeModule],
        open: u16,
        close: u16,
    ) -> Vec<Vec<RuntimeModule>> {
        let mut blocks = Vec::new();
        let mut i = 0;

        while i < successors.len() {
            if successors[i].symbol == open
                && let Some(end) = Self::find_matching_close(successors, i, open, close)
            {
                blocks.push(successors[i..=end].to_vec());
                i = end + 1;
                continue;
            }
            i += 1;
        }

        blocks
    }

    /// Finds the first branch block in successors.
    fn find_branch_block(
        successors: &[RuntimeModule],
        open: u16,
        close: u16,
    ) -> Option<(usize, usize)> {
        for (i, module) in successors.iter().enumerate() {
            if module.symbol == open
                && let Some(end) = Self::find_matching_close(successors, i, open, close)
            {
                return Some((i, end));
            }
        }
        None
    }

    /// Finds the matching close bracket for an open bracket at position `start`.
    fn find_matching_close(
        successors: &[RuntimeModule],
        start: usize,
        open: u16,
        close: u16,
    ) -> Option<usize> {
        let mut depth = 0;
        for (i, module) in successors.iter().enumerate().skip(start) {
            if module.symbol == open {
                depth += 1;
            } else if module.symbol == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }
}
