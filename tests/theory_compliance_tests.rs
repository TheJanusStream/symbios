use symbios::SymbiosState;
use symbios::parser::{
    ast::{Directive, Expr},
    parse_directive, parse_expr, parse_module, parse_rule,
};

#[test]
fn test_il_system_context_words() {
    let input = "A B < C > D E -> F";
    let (_, rule) = parse_rule(input).expect("Should parse word contexts");

    assert_eq!(rule.left_context.len(), 2);
    assert_eq!(rule.left_context[1].symbol, "B");

    assert_eq!(rule.right_context.len(), 2);
    assert_eq!(rule.right_context[0].symbol, "D");
    assert_eq!(rule.right_context[1].symbol, "E");

    assert_eq!(rule.successors.len(), 1);
    assert_eq!(rule.successors[0].symbol, "F");
}

#[test]
fn test_simultaneous_topology_pairs() {
    let mut state = SymbiosState::new();
    let (b_open, b_close) = (1, 2);
    let (p_open, p_close) = (3, 4);

    state.push(b_open, 0.0, &[]).unwrap(); // 0: [
    state.push(p_open, 0.0, &[]).unwrap(); // 1: {
    state.push(p_close, 0.0, &[]).unwrap(); // 2: }
    state.push(b_close, 0.0, &[]).unwrap(); // 3: ]

    state.calculate_topology(b_open, b_close).unwrap();
    state.calculate_topology(p_open, p_close).unwrap();

    // Check that polygon pass did not erase the branch indices
    assert_eq!(state.get_view(0).unwrap().skip_idx, Some(3));
    assert_eq!(state.get_view(3).unwrap().skip_idx, Some(0));

    // Check new polygon links
    assert_eq!(state.get_view(1).unwrap().skip_idx, Some(2));
}

#[test]
fn test_ignore_and_define_directives() {
    let input_ignore = "#ignore : + - F";
    let (_, d1) = parse_directive(input_ignore).expect("Should parse ignore");
    if let Directive::Ignore(symbols) = d1 {
        assert_eq!(symbols, vec!["+", "-", "F"]);
    } else {
        panic!("Wrong directive type");
    }

    let input_define = "#define ANGLE 90";
    let (_, d2) = parse_directive(input_define).expect("Should parse define");
    if let Directive::Define(name, expr) = d2 {
        assert_eq!(name, "ANGLE");
        assert!(matches!(expr, Expr::Number(90.0)));
    } else {
        panic!("Wrong directive type");
    }
}

#[test]
fn test_epsilon_erasing_rule() {
    let input = "A -> ";
    let (_, rule) = parse_rule(input).expect("Should parse erasing rule");
    assert!(rule.successors.is_empty());
}

#[test]
fn test_successor_tokenization_tight() {
    let input = "A -> F+F-F";
    let (_, rule) = parse_rule(input).expect("Should parse tight successors");
    assert_eq!(rule.successors.len(), 5);
    assert_eq!(rule.successors[1].symbol, "+");
}

#[test]
fn test_missing_math_operators() {
    let input = "A(2 ^ 3)";
    let (_, module) = parse_module(input).expect("Should parse ^");
    if let Expr::Pow(lhs, rhs) = &module.params[0] {
        assert!(matches!(**lhs, Expr::Number(2.0)));
        assert!(matches!(**rhs, Expr::Number(3.0)));
    } else {
        panic!("Expected Pow");
    }
}

#[test]
fn test_missing_logical_operators() {
    let input = "t == 1 & s >= 6";
    let (_, expr) = parse_expr(input).expect("Should parse & and >=");
    assert!(matches!(expr, Expr::And(_, _)));
}

#[test]
fn test_named_modules_abop() {
    let input = "internode(1.5)";
    let (_, m) = parse_module(input).expect("Should parse named modules");
    assert_eq!(m.symbol, "internode");
    assert_eq!(m.params.len(), 1);
}

#[test]
fn test_c_style_comments() {
    let input = "/* header */ #define A 1 // tail";
    let (_, d) = parse_directive(input).expect("Should parse through comments");
    if let Directive::Define(n, _) = d {
        assert_eq!(n, "A");
    }
}

#[test]
fn test_ignore_colon_syntax() {
    let input = "#ignore : + - F";
    let (_, d) = parse_directive(input).expect("Should require colon in ignore");
    if let Directive::Ignore(s) = d {
        assert_eq!(s.len(), 3);
    }
}

#[test]
fn test_rule_label_ambiguity() {
    let input = "label1: A -> B";
    let (_, rule) = parse_rule(input).expect("Should handle labels");
    assert_eq!(rule.label, Some("label1".to_string()));
}

#[test]
fn test_topology_sentinel_collision() {
    let mut state = SymbiosState::new();
    let open = 1;
    // Fill state...
    state.push(open, 0.0, &[]).unwrap();
    assert!(state.get_view(0).unwrap().skip_idx.is_none());
}

#[test]
fn test_abop_wildcard_condition() {
    // Validates that 'A : * -> ...' syntax used in ABOP examples works
    let input = "p1 : A : * -> A";
    let (_, rule) = parse_rule(input).expect("Should parse ABOP wildcard style");
    // Should be parsed as condition = 1.0 (True)
    assert_eq!(rule.condition, Some(Expr::Number(1.0)));
}

#[test]
fn test_abop_equality_syntax() {
    // Validates that 't=0' is treated as equality check, not assignment
    let input = "p2 : A(t) : t=1 -> A(t+1)";
    let (_, rule) = parse_rule(input).expect("Should parse ABOP equality alias");
    match rule.condition {
        Some(Expr::Eq(_, _)) => (), // OK
        _ => panic!("Expected Eq operator for '='"),
    }
}
