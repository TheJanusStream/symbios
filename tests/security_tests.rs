use symbios::System;
use symbios::parser::{ast::Expr, parse_expr, parse_module};
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
    // recursive drop is safe at depth 100
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
