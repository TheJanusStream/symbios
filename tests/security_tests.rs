use symbios::System;
use symbios::parser::{ast::Expr, parse_expr, parse_module, parse_rule};
use symbios::{SymbiosState, core::SymbiosError};

#[test]
fn test_3d_symbols() {
    let (_, m) = parse_module("\\(90)").expect("Should parse roll");
    assert_eq!(m.symbol, "\\");
    let (_, m) = parse_module("!(5)").expect("Should parse width");
    assert_eq!(m.symbol, "!");
}

#[test]
fn test_alphanumeric_identifiers() {
    let input = "R(angle_1, x0)";
    let (_, m) = parse_module(input).expect("Should parse alphanumeric ids");
    assert_eq!(m.params.len(), 2);
}

#[test]
fn test_identifier_limit() {
    let long_id = "a".repeat(65);
    let input = format!("A({})", long_id);
    assert!(parse_module(&input).is_err());
}

#[test]
fn test_arg_limit() {
    let args = "1,".repeat(32) + "1";
    let input = format!("A({})", args);
    assert!(parse_module(&input).is_err());
}

#[test]
fn test_nan_rejection() {
    assert!(parse_module("A(NaN)").is_err());
    assert!(parse_module("A(inf)").is_err());
}

#[test]
fn test_call_ambiguity_cut() {
    let input = "func(a";
    let res = parse_expr(input);
    assert!(matches!(res, Err(nom::Err::Failure(_))));
}

#[test]
fn test_nested_call_stack_safety() {
    let mut input = "1".to_string();
    for _ in 0..70 {
        input = format!("f({})", input);
    }
    assert!(parse_expr(&input).is_err());
}

#[test]
fn test_equality_operator() {
    let (_, expr) = parse_expr("a == b").expect("Should parse ==");
    assert!(matches!(expr, Expr::Eq(_, _)));
}

#[test]
fn test_topology_ambiguity_guard() {
    let mut state = SymbiosState::new();
    assert_eq!(
        state.calculate_topology(1, 1),
        Err(SymbiosError::AmbiguousTopology)
    );
}

#[test]
fn test_stack_depth_limit() {
    // Manually construct a 100-deep tree.
    // This is safe to drop but should be rejected by the parser if it were
    // to originate from input.
    let mut expr = Box::new(Expr::Number(1.0));
    for _ in 0..100 {
        expr = Box::new(Expr::Add(expr, Box::new(Expr::Number(1.0))));
    }
    // iterative Drop handles this safely
}

#[test]
fn test_deep_ast_iterative_drop() {
    // Build a left-leaning chain of 20,000 additions.
    // Without iterative Drop, this would stack-overflow on drop.
    let mut expr = Box::new(Expr::Number(1.0));
    for _ in 0..20_000 {
        expr = Box::new(Expr::Add(expr, Box::new(Expr::Number(1.0))));
    }
    drop(expr); // must not stack-overflow
}

#[test]
fn test_deep_ast_iterative_clone_compile_format() {
    // Adversarial parser input — `1 + 1 + ... + 1` parses iteratively into a
    // 20,000-deep left-nested Add chain. Clone, compilation and Display all
    // used to recurse along the spine and stack-overflow. They must now
    // run iteratively.
    let mut expr = Expr::Number(1.0);
    for _ in 0..20_000 {
        expr = Expr::Add(Box::new(expr), Box::new(Expr::Number(1.0)));
    }

    // Clone must not overflow.
    let cloned = expr.clone();

    // Display (compiler-bound surface used by to_source) must not overflow.
    let _rendered = cloned.to_string();

    // Compilation to bytecode must not overflow either.
    use std::collections::HashMap;
    use symbios::vm::Compiler;
    let constants: HashMap<String, f64> = HashMap::new();
    let mut compiler = Compiler::new(vec![], &constants);
    let ops = compiler.compile(&cloned).expect("compile should succeed");
    // Each Add contributes 1 op; plus 20_001 Push ops for the literals.
    assert_eq!(ops.len(), 40_001);
}

#[test]
fn test_power_right_assoc_round_trip() {
    // (a ^ b) ^ c must keep its parens, otherwise it re-parses as a^(b^c).
    let expr = Expr::Pow(
        Box::new(Expr::Pow(
            Box::new(Expr::Variable("a".into())),
            Box::new(Expr::Variable("b".into())),
        )),
        Box::new(Expr::Variable("c".into())),
    );
    let rendered = expr.to_string();
    assert_eq!(rendered, "(a ^ b) ^ c");

    // Round-trip: re-parse must reconstruct (a^b)^c, not a^(b^c).
    let (_, parsed) = parse_expr(&rendered).expect("must re-parse");
    assert_eq!(parsed, expr);
}

#[test]
fn test_context_param_shadowing_prevention() {
    let mut sys = System::new();
    // A(x) defines 'x'. B(x) attempts to redefine 'x'.
    // If successful, C(x) would be ambiguous (is it A's x or B's x?).
    // The system should now reject this rule.
    let res = sys.add_rule("A(x) < B(x) -> C(x)");
    assert!(res.is_err());

    // Correct usage: unique names
    let res_valid = sys.add_rule("A(x) < B(y) -> C(x + y)");
    assert!(res_valid.is_ok());
}

#[test]
fn test_context_length_limit() {
    // 33 context modules exceeds MAX_CONTEXT_LENGTH (32)
    let left_ctx: Vec<&str> = (0..33).map(|_| "A").collect();
    let rule_str = format!("{} < B -> C", left_ctx.join(" "));
    assert!(
        parse_rule(&rule_str).is_err(),
        "Left context exceeding MAX_CONTEXT_LENGTH should be rejected"
    );

    // Right context similarly bounded
    let right_ctx: Vec<&str> = (0..33).map(|_| "A").collect();
    let rule_str = format!("B > {} -> C", right_ctx.join(" "));
    assert!(
        parse_rule(&rule_str).is_err(),
        "Right context exceeding MAX_CONTEXT_LENGTH should be rejected"
    );

    // At the limit (32) should succeed
    let left_ctx: Vec<&str> = (0..32).map(|_| "A").collect();
    let rule_str = format!("{} < B -> C", left_ctx.join(" "));
    assert!(
        parse_rule(&rule_str).is_ok(),
        "Left context at MAX_CONTEXT_LENGTH should be accepted"
    );
}

#[test]
fn test_zero_probability_no_panic() {
    let mut sys = System::new();
    sys.set_axiom("A").unwrap();
    // Two rules both with probability 0 — derivation must not panic
    sys.add_rule("0.0 : A -> B").unwrap();
    sys.add_rule("0.0 : A -> C").unwrap();
    // Should fall through to identity (no matching rule selected)
    let result = sys.derive(1);
    assert!(result.is_ok(), "Zero-probability rules must not panic");
}
