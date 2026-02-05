pub mod crossover;
pub mod derivation;
pub mod export;
pub mod matching;
pub mod mutate;

use crate::core::SymbiosState;
use crate::core::interner::SymbolTable;
use crate::parser::{self, ast};
use crate::vm::{Compiler, Op};
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use std::collections::HashMap;
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
                    if param_names.contains(name) {
                        return Err(SystemError::CompileError(format!(
                            "Shadowing or ambiguous parameter in left context: {}",
                            name
                        )));
                    }
                    param_names.push(name.clone());
                }
            }
        }

        for m in &rule_ast.right_context {
            expected_arities.push(m.params.len());
            for param in &m.params {
                if let ast::Expr::Variable(name) = param {
                    if param_names.contains(name) {
                        return Err(SystemError::CompileError(format!(
                            "Shadowing or ambiguous parameter in right context: {}",
                            name
                        )));
                    }
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
