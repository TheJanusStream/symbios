//! Source code export functionality for RuntimeRules.
//!
//! This module provides the ability to convert compiled RuntimeRules back into
//! AST representations and source text. This is useful for:
//! - Displaying mutated rules to users after evolutionary operations
//! - Serializing rules in human-readable form
//! - Debugging and inspection

use crate::core::interner::SymbolTable;
use crate::parser::ast::{Expr, ModuleSym, Rule};
use crate::system::{RuntimeModule, RuntimeRule};
use crate::vm::decompile_with_params;

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
    let condition = if let Some(cond_bytecode) = &rule.condition {
        Some(decompile_with_params(cond_bytecode, &param_map)?)
    } else {
        None
    };

    // Export successors
    let mut successors = Vec::new();
    for module in &rule.successors {
        let succ = export_module(module, interner, &param_map)?;
        successors.push(succ);
    }

    Ok(Rule {
        label: None, // Labels are not preserved in RuntimeRule
        probability: rule.probability,
        predecessor,
        left_context,
        right_context,
        condition,
        successors,
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
        };

        let config = ExportConfig::default();
        let result = export_rule_to_string(&rule, &interner, &config).unwrap();
        assert_eq!(result, "A -> B : 0.5");
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
        };

        let config = ExportConfig::synthetic(&rule);
        assert_eq!(config.predecessor_params, vec!["p0", "p1"]);
        assert_eq!(config.left_context_params, vec!["l0_0"]);
        assert_eq!(
            config.right_context_params,
            vec!["r0_0", "r0_1", "r0_2"]
        );
    }
}
