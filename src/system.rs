use crate::core::SymbiosState;
use crate::core::interner::SymbolTable;
use crate::parser::{self, ast};
use crate::vm::{Compiler, Op};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use std::collections::HashMap;
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
    State(#[from] crate::core::SymbiosError),
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
    /// Stochastic probability (0.0 - 1.0).
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
    /// Stored initial axiom state for reset functionality.
    initial_state: Option<SymbiosState>,
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
            initial_state: None,
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

                let mut candidates: Vec<&RuntimeRule> = Vec::new();
                let mut total_probability = 0.0;

                if let Some(bucket) = self.rules.get(&view.sym) {
                    for rule in bucket {
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
                            candidates.push(rule);
                            total_probability += rule.probability;
                        }
                    }
                }

                let selected_rule = if candidates.is_empty() || total_probability <= 0.0 {
                    None
                } else if candidates.len() == 1 {
                    Some(candidates[0])
                } else {
                    let mut r = self.rng.random_range(0.0..total_probability);
                    let mut winner = None;
                    for rule in &candidates {
                        if r < rule.probability {
                            winner = Some(*rule);
                            break;
                        }
                        r -= rule.probability;
                    }
                    winner.or_else(|| candidates.last().copied())
                };

                if let Some(rule) = selected_rule {
                    // Clear and reuse generation buffers
                    self.gen_context_frame.clear();
                    self.gen_context_frame.extend_from_slice(view.params);

                    if !rule.left_context.is_empty() {
                        self.gen_left_indices.clear();
                        matching::match_left(
                            &self.state,
                            index,
                            &rule.left_context,
                            &self.ignored_symbols,
                            &mut self.gen_left_indices,
                        );
                        for &i in &self.gen_left_indices {
                            self.gen_context_frame
                                .extend_from_slice(self.state.get_view(i).unwrap().params);
                        }
                    }

                    if !rule.right_context.is_empty() {
                        self.gen_right_indices.clear();
                        matching::match_right(
                            &self.state,
                            index,
                            &rule.right_context,
                            &self.ignored_symbols,
                            &mut self.gen_right_indices,
                        );
                        for &i in &self.gen_right_indices {
                            self.gen_context_frame
                                .extend_from_slice(self.state.get_view(i).unwrap().params);
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

        let new_rule = RuntimeRule {
            predecessor: pred_sym,
            left_context: left_ctx,
            right_context: right_ctx,
            probability: rule_ast.probability,
            condition: condition_code,
            successors: runtime_successors,
            expected_arities,
        };

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
                scratch
                    .context_frame
                    .extend_from_slice(state.get_view(i).unwrap().params);
            }
            for &i in &scratch.right_indices {
                scratch
                    .context_frame
                    .extend_from_slice(state.get_view(i).unwrap().params);
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
            let view = state.get_view(curr as usize).unwrap();

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
            initial_state: self.initial_state.clone(),
        }
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
}

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            rule_probability_rate: 0.1,
            rule_probability_strength: 0.2,
            constant_rate: 0.1,
            constant_strength: 0.2,
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
        // Mutate rule probabilities
        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if self.rng.random::<f64>() < config.rule_probability_rate {
                    let delta = self.rng.random_range(
                        -config.rule_probability_strength..=config.rule_probability_strength,
                    );
                    rule.probability = (rule.probability + delta).clamp(0.0, 1.0);
                }
            }
        }

        // Mutate constants
        let constant_keys: Vec<String> = self.constants.keys().cloned().collect();
        for key in constant_keys {
            if self.rng.random::<f64>() < config.constant_rate {
                if let Some(val) = self.constants.get_mut(&key) {
                    let factor = 1.0
                        + self
                            .rng
                            .random_range(-config.constant_strength..=config.constant_strength);
                    *val *= factor;
                }
            }
        }
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
    /// A new `System` combining genetic material from both parents.
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
    /// let offspring = parent_a.crossover(&parent_b, &config);
    /// ```
    pub fn crossover(&mut self, other: &System, config: &CrossoverConfig) -> System {
        let mut offspring = System::new();
        offspring.rng = Pcg64::seed_from_u64(self.rng.random());
        offspring.max_capacity = self.max_capacity.max(other.max_capacity);
        offspring.ignored_symbols = self.ignored_symbols.clone();

        // Merge interners - collect all symbols from both parents
        let mut symbol_map_self: HashMap<u16, u16> = HashMap::new();
        let mut symbol_map_other: HashMap<u16, u16> = HashMap::new();

        for (old_id, name) in self.interner.iter() {
            let new_id = offspring.interner.get_or_intern(name).unwrap_or(old_id);
            symbol_map_self.insert(old_id, new_id);
        }

        for (old_id, name) in other.interner.iter() {
            let new_id = offspring.interner.get_or_intern(name).unwrap_or(old_id);
            symbol_map_other.insert(old_id, new_id);
        }

        // Collect all predecessor symbols from both parents
        let mut all_predecessors: Vec<u16> = Vec::new();
        for &pred in self.rules.keys() {
            if let Some(&new_pred) = symbol_map_self.get(&pred) {
                if !all_predecessors.contains(&new_pred) {
                    all_predecessors.push(new_pred);
                }
            }
        }
        for &pred in other.rules.keys() {
            if let Some(&new_pred) = symbol_map_other.get(&pred) {
                if !all_predecessors.contains(&new_pred) {
                    all_predecessors.push(new_pred);
                }
            }
        }

        // For each predecessor, select rules from one parent or the other
        for new_pred in all_predecessors {
            // Find original predecessor IDs
            let self_pred = symbol_map_self
                .iter()
                .find(|(_, v)| **v == new_pred)
                .map(|(k, _)| *k);
            let other_pred = symbol_map_other
                .iter()
                .find(|(_, v)| **v == new_pred)
                .map(|(k, _)| *k);

            let self_rules = self_pred.and_then(|p| self.rules.get(&p));
            let other_rules = other_pred.and_then(|p| other.rules.get(&p));

            let (selected_rules, symbol_map) = match (self_rules, other_rules) {
                (Some(rules), None) => (rules, &symbol_map_self),
                (None, Some(rules)) => (rules, &symbol_map_other),
                (Some(self_r), Some(other_r)) => {
                    if self.rng.random::<f64>() < config.rule_bias {
                        (self_r, &symbol_map_self)
                    } else {
                        (other_r, &symbol_map_other)
                    }
                }
                (None, None) => continue,
            };

            // Remap and insert rules
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
                (Some(a), Some(b)) => a * (1.0 - config.constant_blend) + b * config.constant_blend,
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => continue,
            };
            offspring.constants.insert(key, blended);
        }

        // Remap ignored symbols
        offspring.ignored_symbols = self
            .ignored_symbols
            .iter()
            .filter_map(|s| symbol_map_self.get(s).copied())
            .collect();

        offspring
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
            if rng.random::<f64>() < config.constant_rate {
                if let Some(val) = self.constants.get_mut(&key) {
                    let factor = 1.0
                        + rng.random_range(-config.constant_strength..=config.constant_strength);
                    *val *= factor;
                }
            }
        }
    }

    /// Performs crossover using an external RNG for reproducibility.
    pub fn crossover_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        config: &CrossoverConfig,
    ) -> System {
        let mut offspring = System::new();
        offspring.rng = Pcg64::seed_from_u64(rng.random());
        offspring.max_capacity = self.max_capacity.max(other.max_capacity);

        let mut symbol_map_self: HashMap<u16, u16> = HashMap::new();
        let mut symbol_map_other: HashMap<u16, u16> = HashMap::new();

        for (old_id, name) in self.interner.iter() {
            let new_id = offspring.interner.get_or_intern(name).unwrap_or(old_id);
            symbol_map_self.insert(old_id, new_id);
        }

        for (old_id, name) in other.interner.iter() {
            let new_id = offspring.interner.get_or_intern(name).unwrap_or(old_id);
            symbol_map_other.insert(old_id, new_id);
        }

        let mut all_predecessors: Vec<u16> = Vec::new();
        for &pred in self.rules.keys() {
            if let Some(&new_pred) = symbol_map_self.get(&pred) {
                if !all_predecessors.contains(&new_pred) {
                    all_predecessors.push(new_pred);
                }
            }
        }
        for &pred in other.rules.keys() {
            if let Some(&new_pred) = symbol_map_other.get(&pred) {
                if !all_predecessors.contains(&new_pred) {
                    all_predecessors.push(new_pred);
                }
            }
        }

        for new_pred in all_predecessors {
            let self_pred = symbol_map_self
                .iter()
                .find(|(_, v)| **v == new_pred)
                .map(|(k, _)| *k);
            let other_pred = symbol_map_other
                .iter()
                .find(|(_, v)| **v == new_pred)
                .map(|(k, _)| *k);

            let self_rules = self_pred.and_then(|p| self.rules.get(&p));
            let other_rules = other_pred.and_then(|p| other.rules.get(&p));

            let (selected_rules, symbol_map) = match (self_rules, other_rules) {
                (Some(rules), None) => (rules, &symbol_map_self),
                (None, Some(rules)) => (rules, &symbol_map_other),
                (Some(self_r), Some(other_r)) => {
                    if rng.random::<f64>() < config.rule_bias {
                        (self_r, &symbol_map_self)
                    } else {
                        (other_r, &symbol_map_other)
                    }
                }
                (None, None) => continue,
            };

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

        offspring.ignored_symbols = self
            .ignored_symbols
            .iter()
            .filter_map(|s| symbol_map_self.get(s).copied())
            .collect();

        offspring
    }
}
