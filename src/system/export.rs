//! Source code export functionality for RuntimeRules.
//!
//! This module provides the ability to convert compiled RuntimeRules back into
//! AST representations and source text. This is useful for:
//! - Displaying mutated rules to users after evolutionary operations
//! - Serializing rules in human-readable form
//! - Debugging and inspection

use super::{System, SystemError};

use crate::core::interner::SymbolTable;
use crate::parser::ast::{Expr, ModuleSym, Rule};
use crate::system::{RuntimeModule, RuntimeRule};
use crate::vm::decompile_with_params;

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
                let config = ExportConfig::from_rule(rule);
                if let Ok(source) = export_rule_to_string(rule, &self.interner, &config) {
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
            let config = ExportConfig::from_rule(rule);
            if let Ok(source) = export_rule_to_string(rule, &self.interner, &config) {
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

        let config = ExportConfig::from_rule(rule);
        export_rule_to_string(rule, &self.interner, &config).map_err(SystemError::CompileError)
    }

    /// Reconstructs a complete L-system source file from the current state.
    ///
    /// Output order: preamble (comments, `#ignore`) → `#define` lines → `omega:` axiom → rules.
    /// Uses stored parameter names for high-fidelity round-tripping.
    ///
    /// # Example
    /// ```
    /// use symbios::System;
    ///
    /// let source = "// A simple system\n#define n 5\nomega: A(n)\nA(x) : x > 0 -> A(x - 1) B";
    /// let sys = System::from_source(source).unwrap();
    /// let output = sys.to_source();
    /// assert!(output.contains("// A simple system"));
    /// assert!(output.contains("#define n 5"));
    /// assert!(output.contains("omega:"));
    /// ```
    pub fn to_source(&self) -> String {
        let mut result = String::new();

        // Emit preamble lines (comments, #ignore — but NOT #define, which we regenerate)
        for line in &self.preamble {
            let trimmed = line.trim();
            if trimmed.starts_with("#define") {
                continue;
            }
            result.push_str(line);
            result.push('\n');
        }

        // Emit #define lines from constants (sorted for determinism)
        let mut constants: Vec<_> = self.constants.iter().collect();
        constants.sort_by_key(|(k, _)| *k);
        for (name, value) in constants {
            result.push_str(&format!("#define {} {}\n", name, value));
        }

        // Emit axiom line
        if let Some(ref axiom) = self.axiom_source {
            result.push_str(axiom);
            result.push('\n');
        }

        // Emit rules with preserved parameter names
        let exported = self.export_rules();
        for (_, rule_source) in exported {
            result.push_str(&rule_source);
            result.push('\n');
        }

        result.trim_end().to_string()
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

        let config = ExportConfig {
            predecessor_params: param_names,
            ..Default::default()
        };

        export_rule_to_string(rule, &self.interner, &config).map_err(SystemError::CompileError)
    }
}

/// Configuration for rule export, including parameter naming.
#[derive(Debug, Clone, Default)]
pub struct ExportConfig {
    /// Parameter names for the predecessor module.
    /// If not provided, synthetic names (p0, p1, ...) are used.
    pub predecessor_params: Vec<String>,
    /// Parameter names for left context modules (flattened).
    pub left_context_params: Vec<String>,
    /// Parameter names for right context modules (flattened).
    pub right_context_params: Vec<String>,
}

impl ExportConfig {
    /// Creates an ExportConfig with synthetic parameter names based on arities.
    ///
    /// Generates names like p0, p1, p2, ... for predecessor,
    /// l0, l1, ... for left context, and r0, r1, ... for right context.
    pub fn synthetic(rule: &RuntimeRule) -> Self {
        let pred_arity = rule.expected_arities.first().copied().unwrap_or(0);
        let predecessor_params: Vec<String> = (0..pred_arity).map(|i| format!("p{}", i)).collect();

        let mut left_context_params = Vec::new();
        for (i, &arity) in rule.expected_arities[1..1 + rule.left_context.len()]
            .iter()
            .enumerate()
        {
            for j in 0..arity {
                left_context_params.push(format!("l{}_{}", i, j));
            }
        }

        let mut right_context_params = Vec::new();
        let right_start = 1 + rule.left_context.len();
        for (i, &arity) in rule.expected_arities[right_start..].iter().enumerate() {
            for j in 0..arity {
                right_context_params.push(format!("r{}_{}", i, j));
            }
        }

        Self {
            predecessor_params,
            left_context_params,
            right_context_params,
        }
    }

    /// Creates config from stored parameter names in the rule.
    /// Falls back to synthetic names if no param names are stored.
    pub fn from_rule(rule: &RuntimeRule) -> Self {
        if rule.param_names.is_empty() {
            return Self::synthetic(rule);
        }

        let pred_arity = rule.expected_arities.first().copied().unwrap_or(0);
        let predecessor_params: Vec<String> = rule.param_names[..pred_arity].to_vec();

        let mut offset = pred_arity;
        let mut left_context_params = Vec::new();
        for &arity in rule.expected_arities[1..1 + rule.left_context.len()].iter() {
            let end = (offset + arity).min(rule.param_names.len());
            left_context_params.extend_from_slice(&rule.param_names[offset..end]);
            offset = end;
        }

        let mut right_context_params = Vec::new();
        let right_start = 1 + rule.left_context.len();
        for &arity in rule.expected_arities[right_start..].iter() {
            let end = (offset + arity).min(rule.param_names.len());
            right_context_params.extend_from_slice(&rule.param_names[offset..end]);
            offset = end;
        }

        Self {
            predecessor_params,
            left_context_params,
            right_context_params,
        }
    }

    /// Builds the complete parameter map for decompilation.
    fn build_param_map(&self) -> Vec<String> {
        let mut params = Vec::new();
        params.extend(self.predecessor_params.iter().cloned());
        params.extend(self.left_context_params.iter().cloned());
        params.extend(self.right_context_params.iter().cloned());
        params
    }
}

/// Exports a RuntimeModule to an AST ModuleSym.
pub fn export_module(
    module: &RuntimeModule,
    interner: &SymbolTable,
    param_names: &[String],
) -> Result<ModuleSym, String> {
    let symbol = interner
        .resolve(module.symbol)
        .ok_or_else(|| format!("Unknown symbol ID: {}", module.symbol))?
        .to_string();

    let mut params = Vec::new();
    for bytecode in &module.params {
        let expr = decompile_with_params(bytecode, param_names)?;
        params.push(expr);
    }

    Ok(ModuleSym { symbol, params })
}

/// Exports a RuntimeRule to an AST Rule.
///
/// This reconstructs the rule structure from compiled bytecode.
/// Parameter names are either provided in the config or generated synthetically.
pub fn export_rule(
    rule: &RuntimeRule,
    interner: &SymbolTable,
    config: &ExportConfig,
) -> Result<Rule, String> {
    let param_map = config.build_param_map();

    // Resolve predecessor symbol
    let pred_symbol = interner
        .resolve(rule.predecessor)
        .ok_or_else(|| format!("Unknown predecessor symbol ID: {}", rule.predecessor))?
        .to_string();

    // Build predecessor params as Variable expressions
    let pred_params: Vec<Expr> = config
        .predecessor_params
        .iter()
        .map(|name| Expr::Variable(name.clone()))
        .collect();

    let predecessor = ModuleSym {
        symbol: pred_symbol,
        params: pred_params,
    };

    // Build left context
    let mut left_context = Vec::new();
    let mut left_param_idx = 0;
    for (i, &sym_id) in rule.left_context.iter().enumerate() {
        let symbol = interner
            .resolve(sym_id)
            .ok_or_else(|| format!("Unknown left context symbol ID: {}", sym_id))?
            .to_string();

        let arity = rule.expected_arities.get(1 + i).copied().unwrap_or(0);
        let params: Vec<Expr> = (0..arity)
            .map(|j| {
                let name = config
                    .left_context_params
                    .get(left_param_idx + j)
                    .cloned()
                    .unwrap_or_else(|| format!("l{}_{}", i, j));
                Expr::Variable(name)
            })
            .collect();
        left_param_idx += arity;

        left_context.push(ModuleSym { symbol, params });
    }

    // Build right context
    let mut right_context = Vec::new();
    let mut right_param_idx = 0;
    let right_start = 1 + rule.left_context.len();
    for (i, &sym_id) in rule.right_context.iter().enumerate() {
        let symbol = interner
            .resolve(sym_id)
            .ok_or_else(|| format!("Unknown right context symbol ID: {}", sym_id))?
            .to_string();

        let arity = rule
            .expected_arities
            .get(right_start + i)
            .copied()
            .unwrap_or(0);
        let params: Vec<Expr> = (0..arity)
            .map(|j| {
                let name = config
                    .right_context_params
                    .get(right_param_idx + j)
                    .cloned()
                    .unwrap_or_else(|| format!("r{}_{}", i, j));
                Expr::Variable(name)
            })
            .collect();
        right_param_idx += arity;

        right_context.push(ModuleSym { symbol, params });
    }

    // Decompile condition
    let mut condition = if let Some(cond_bytecode) = &rule.condition {
        Some(decompile_with_params(cond_bytecode, &param_map)?)
    } else {
        None
    };

    // Sync numeric condition with probability for consistent export
    if let Some(Expr::Number(_)) = condition {
        condition = Some(Expr::Number(rule.probability));
    }

    // Export successors
    let mut successors = Vec::new();
    for module in &rule.successors {
        let succ = export_module(module, interner, &param_map)?;
        successors.push(succ);
    }

    // Round-trip per-rule ignore list: resolve each interned id back to its
    // string form. If a symbol is unknown to the interner we silently drop
    // it — that should never happen in practice (interner is monotonic) but
    // we'd rather export a slightly lossy rule than abort.
    let ignored_symbols = rule.ignored_symbols.as_ref().map(|ids| {
        ids.iter()
            .filter_map(|&id| interner.resolve(id).map(|s| s.to_string()))
            .collect::<Vec<_>>()
    });

    Ok(Rule {
        label: None, // Labels are not preserved in RuntimeRule
        probability: rule.probability,
        predecessor,
        left_context,
        right_context,
        condition,
        successors,
        ignored_symbols,
    })
}

/// Exports a RuntimeRule to source text.
///
/// This is a convenience function that exports to AST and then formats.
pub fn export_rule_to_string(
    rule: &RuntimeRule,
    interner: &SymbolTable,
    config: &ExportConfig,
) -> Result<String, String> {
    let ast_rule = export_rule(rule, interner, config)?;
    Ok(ast_rule.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::Op;

    fn setup_interner() -> SymbolTable {
        let mut interner = SymbolTable::new();
        interner.get_or_intern("A").unwrap();
        interner.get_or_intern("B").unwrap();
        interner.get_or_intern("C").unwrap();
        interner
    }

    #[test]
    fn test_export_simple_rule() {
        let interner = setup_interner();
        let a_id = interner.resolve_id("A").unwrap();
        let b_id = interner.resolve_id("B").unwrap();

        let rule = RuntimeRule {
            predecessor: a_id,
            left_context: vec![],
            right_context: vec![],
            probability: 1.0,
            condition: None,
            successors: vec![
                RuntimeModule {
                    symbol: a_id,
                    params: vec![],
                },
                RuntimeModule {
                    symbol: b_id,
                    params: vec![],
                },
            ],
            expected_arities: vec![0],
            param_names: vec![],
            ignored_symbols: None,
        };

        let config = ExportConfig::default();
        let result = export_rule_to_string(&rule, &interner, &config).unwrap();
        assert_eq!(result, "A -> A B");
    }

    #[test]
    fn test_export_parametric_rule() {
        let interner = setup_interner();
        let a_id = interner.resolve_id("A").unwrap();
        let b_id = interner.resolve_id("B").unwrap();

        // A(x) : x > 10 -> B(x + 1)
        let rule = RuntimeRule {
            predecessor: a_id,
            left_context: vec![],
            right_context: vec![],
            probability: 1.0,
            condition: Some(vec![Op::LoadParam(0), Op::Push(10.0), Op::Gt]),
            successors: vec![RuntimeModule {
                symbol: b_id,
                params: vec![vec![Op::LoadParam(0), Op::Push(1.0), Op::Add]],
            }],
            expected_arities: vec![1],
            param_names: vec!["x".into()],
            ignored_symbols: None,
        };

        let config = ExportConfig {
            predecessor_params: vec!["x".into()],
            ..Default::default()
        };

        let result = export_rule_to_string(&rule, &interner, &config).unwrap();
        assert_eq!(result, "A(x) : x > 10 -> B(x + 1)");
    }

    #[test]
    fn test_export_context_rule() {
        let interner = setup_interner();
        let a_id = interner.resolve_id("A").unwrap();
        let b_id = interner.resolve_id("B").unwrap();
        let c_id = interner.resolve_id("C").unwrap();

        // A < B > C -> C
        let rule = RuntimeRule {
            predecessor: b_id,
            left_context: vec![a_id],
            right_context: vec![c_id],
            probability: 1.0,
            condition: None,
            successors: vec![RuntimeModule {
                symbol: c_id,
                params: vec![],
            }],
            expected_arities: vec![0, 0, 0],
            param_names: vec![],
            ignored_symbols: None,
        };

        let config = ExportConfig::default();
        let result = export_rule_to_string(&rule, &interner, &config).unwrap();
        assert_eq!(result, "A < B > C -> C");
    }

    #[test]
    fn test_export_stochastic_rule() {
        let interner = setup_interner();
        let a_id = interner.resolve_id("A").unwrap();
        let b_id = interner.resolve_id("B").unwrap();

        let rule = RuntimeRule {
            predecessor: a_id,
            left_context: vec![],
            right_context: vec![],
            probability: 0.5,
            condition: None,
            successors: vec![RuntimeModule {
                symbol: b_id,
                params: vec![],
            }],
            expected_arities: vec![0],
            param_names: vec![],
            ignored_symbols: None,
        };

        let config = ExportConfig::default();
        let result = export_rule_to_string(&rule, &interner, &config).unwrap();
        assert_eq!(result, "0.5 : A -> B");
    }

    #[test]
    fn test_synthetic_config() {
        let rule = RuntimeRule {
            predecessor: 0,
            left_context: vec![1],
            right_context: vec![2],
            probability: 1.0,
            condition: None,
            successors: vec![],
            expected_arities: vec![2, 1, 3], // pred has 2 params, left has 1, right has 3
            param_names: vec![],
            ignored_symbols: None,
        };

        let config = ExportConfig::synthetic(&rule);
        assert_eq!(config.predecessor_params, vec!["p0", "p1"]);
        assert_eq!(config.left_context_params, vec!["l0_0"]);
        assert_eq!(config.right_context_params, vec!["r0_0", "r0_1", "r0_2"]);
    }
}
