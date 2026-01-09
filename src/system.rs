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
    #[error("Invalid predecessor parameter: must be a variable")]
    InvalidPredecessorParam,
}

/// A module in the Successor list.
/// Instead of raw values, it contains Bytecode to BE evaluated to produce values.
#[derive(Debug, Clone)]
pub struct RuntimeModule {
    pub symbol: u16,
    pub params: Vec<Vec<Op>>, // Each param is a compiled expression
}

/// A rule optimized for the derivation engine.
#[derive(Debug, Clone)]
pub struct RuntimeRule {
    pub predecessor: u16,
    pub probability: f64,
    pub condition: Option<Vec<Op>>,
    pub successors: Vec<RuntimeModule>,
    // Debug info: stores the number of variables this rule binds (e.g. A(x,y) = 2)
    pub param_count: usize,
}

pub struct System {
    pub interner: SymbolTable,
    pub rules: Vec<RuntimeRule>,
    // The current state of the L-System (Axiom/Derivation result)
    pub state: SymbiosState,
}

impl System {
    pub fn new() -> Self {
        Self {
            interner: SymbolTable::new(),
            rules: Vec::new(),
            state: SymbiosState::new(),
        }
    }

    /// Parses a single rule string and adds it to the system.
    /// Example: "A(x, y) : y < 5 -> A(x * 2, y + 1)"
    pub fn add_rule(&mut self, rule_src: &str) -> Result<(), SystemError> {
        // 1. Parse
        let (_, rule_ast) =
            parser::parse_rule(rule_src).map_err(|e| SystemError::ParseError(e.to_string()))?;

        // 2. Extract Variable Names from Predecessor (Variable Binding)
        // e.g. A(t, d) -> ["t", "d"]
        let mut param_names = Vec::new();
        for param in &rule_ast.predecessor.params {
            match param {
                ast::Expr::Variable(name) => param_names.push(name.clone()),
                _ => return Err(SystemError::InvalidPredecessorParam),
            }
        }
        let param_count = param_names.len();

        // 3. Initialize Compiler with these bindings
        // This maps "t" -> LoadParam(0), "d" -> LoadParam(1)
        let mut compiler = Compiler::new(param_names);

        // 4. Intern Predecessor Symbol
        let pred_sym = self.interner.intern(&rule_ast.predecessor.symbol);

        // 5. Compile Condition (if exists)
        let condition_code = if let Some(cond_expr) = &rule_ast.condition {
            Some(
                compiler
                    .compile(cond_expr)
                    .map_err(SystemError::CompileError)?,
            )
        } else {
            None
        };

        // 6. Compile Successors
        let mut runtime_successors = Vec::new();
        for succ in &rule_ast.successors {
            let succ_sym = self.interner.intern(&succ.symbol);

            let mut compiled_params = Vec::new();
            for expr in &succ.params {
                let code = compiler.compile(expr).map_err(SystemError::CompileError)?;
                compiled_params.push(code);
            }

            runtime_successors.push(RuntimeModule {
                symbol: succ_sym,
                params: compiled_params,
            });
        }

        // 7. Store
        self.rules.push(RuntimeRule {
            predecessor: pred_sym,
            probability: rule_ast.probability,
            condition: condition_code,
            successors: runtime_successors,
            param_count,
        });

        Ok(())
    }

    /// Sets the axiom from a string string.
    /// Example: "A(10, 5) B(0)"
    pub fn set_axiom(&mut self, axiom_src: &str) -> Result<(), SystemError> {
        let mut remaining = axiom_src;
        self.state.clear();

        while !remaining.trim().is_empty() {
            let (next_input, module) = parser::parse_module(remaining)
                .map_err(|e| SystemError::ParseError(e.to_string()))?;

            let sym_id = self.interner.intern(&module.symbol);

            // For the axiom, expressions must be constant numbers (literals).
            // We do a simple evaluation here or strict check.
            let mut values = Vec::new();
            for expr in module.params {
                if let ast::Expr::Number(val) = expr {
                    values.push(val);
                } else {
                    return Err(SystemError::CompileError(
                        "Axiom parameters must be literals".into(),
                    ));
                }
            }

            // Push to SymbiosState
            // We ignore errors here for brevity, but in prod we'd map SymbiosError
            let _ = self.state.push(sym_id, &values);

            remaining = next_input;
        }

        Ok(())
    }
}
