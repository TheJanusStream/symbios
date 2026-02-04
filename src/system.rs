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
                            let ctx_view = self
                                .state
                                .get_view(i)
                                .ok_or(SystemError::StateCorruption(i))?;
                            self.gen_context_frame.extend_from_slice(ctx_view.params);
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

        for new_pred in all_predecessors {
            let self_pred = reverse_map_self.get(new_pred as usize).copied().flatten();
            let other_pred = reverse_map_other.get(new_pred as usize).copied().flatten();

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

        // Merge symbol_arities from both parents (fixes genetic metadata corruption)
        // Map old symbol IDs to new IDs and merge arities
        for (&old_id, &arity) in &self.symbol_arities {
            if let Some(&new_id) = symbol_map_self.get(&old_id) {
                offspring.symbol_arities.insert(new_id, arity);
            }
        }
        for (&old_id, &arity) in &other.symbol_arities {
            if let Some(&new_id) = symbol_map_other.get(&old_id) {
                // Only insert if not already present (self takes precedence)
                offspring.symbol_arities.entry(new_id).or_insert(arity);
            }
        }

        Ok(offspring)
    }
}
