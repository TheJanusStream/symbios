use nom::error::ErrorKind;
use symbios::parser;
use symbios::parser::{ast::Expr, parse_module, parse_rule};

#[test]
fn test_operator_precedence_climbing() {
    let input = "A(2 * 3 + 4)";
    let (_, module) = parse_module(input).expect("Should parse");
    let expr = &module.params[0];

    if let Expr::Add(lhs, rhs) = expr {
        if let Expr::Mul(m_lhs, m_rhs) = &**lhs {
            assert_eq!(**m_lhs, Expr::Number(2.0));
            assert_eq!(**m_rhs, Expr::Number(3.0));
        } else {
            panic!("LHS of Add should be Mul, got {:?}", lhs);
        }
        assert_eq!(**rhs, Expr::Number(4.0));
    } else {
        panic!("Root expression should be Add, got {:?}", expr);
    }
}

#[test]
fn test_abop_precedence_neg_pow() {
    // ABOP p. 53: -2^2 must be -(2^2) = -4
    let input = "A(-2^2)";
    let (_, module) = parse_module(input).expect("Should parse");
    let expr = &module.params[0];

    // Root should be Negation
    assert!(
        matches!(expr, Expr::Neg(_)),
        "Root should be Negation, got {:?}",
        expr
    );
    if let Expr::Neg(inner) = expr {
        // Inner should be 2^2
        assert!(matches!(**inner, Expr::Pow(_, _)), "Inner should be Power");
    }
}

#[test]
fn test_robust_float_parsing() {
    let input = "T(1.5, 0.5, 10.0, 1e-2, -5.0)";
    let (_, module) = parse_module(input).expect("Should parse floats");

    fn eval(e: &Expr) -> f64 {
        match e {
            Expr::Number(n) => *n,
            Expr::Neg(inner) => {
                if let Expr::Number(n) = &**inner {
                    -n
                } else {
                    panic!("Expected number in Neg")
                }
            }
            _ => panic!("Expected numeric literal"),
        }
    }

    let values: Vec<f64> = module.params.iter().map(eval).collect();
    assert_eq!(values, vec![1.5, 0.5, 10.0, 0.01, -5.0]);
}

#[test]
fn test_function_calls_and_variables() {
    let input = "F(sin(x), cos(t * 0.1))";
    let (_, module) = parse_module(input).expect("Should parse calls");

    match &module.params[0] {
        Expr::Call(name, args) => {
            assert_eq!(name, "sin");
            assert_eq!(args.len(), 1);
            assert!(matches!(args[0], Expr::Variable(_)));
        }
        _ => panic!("Expected Call for param 1"),
    }
}

#[test]
fn test_full_context_rule() {
    let input = "A(x) < B(y) > C : y > 5 -> B(y+1)";
    let (_, rule) = parse_rule(input).expect("Should parse rule");

    assert_eq!(rule.predecessor.symbol, "B");
    assert_eq!(rule.left_context[0].symbol, "A");
    assert_eq!(rule.right_context[0].symbol, "C");
    assert!(matches!(rule.condition, Some(Expr::Gt(_, _))));
}

#[test]
fn test_valid_op_chain() {
    let mut long_expr = "1".to_string();
    for _ in 0..50 {
        long_expr.push_str(" + 1");
    }
    let input = format!("A({})", long_expr);
    assert!(parse_module(&input).is_ok());
}

#[test]
fn test_recursion_depth_limit() {
    // Generate nesting deeper than MAX_RECURSION_DEPTH (64)
    let mut input = "1".to_string();
    for _ in 0..70 {
        input = format!("({})", input);
    }

    let result = parser::parse_expr(&input);

    match result {
        Err(nom::Err::Failure(e)) | Err(nom::Err::Error(e)) => {
            assert_eq!(e.code, ErrorKind::TooLarge);
        }
        Ok(_) => panic!("Parser should have rejected deep nesting"),
        _ => panic!("Unexpected error type"),
    }
}

#[test]
fn test_iterative_long_chain() {
    // 100 operators should PASS in an iterative parser (Stack Safety Check)
    let mut input = "1".to_string();
    for _ in 0..100 {
        input.push_str(" + 1");
    }

    let result = parser::parse_expr(&input);
    assert!(
        result.is_ok(),
        "Iterative parser should handle long flat chains"
    );
}

#[test]
fn test_wildcard_condition_parsing() {
    // Tests that '*' is parsed as 1.0 (True)
    let input = "A : * -> B";
    let (_, rule) = parse_rule(input).expect("Should parse wildcard condition");
    assert_eq!(rule.condition, Some(Expr::Number(1.0)));
}

#[test]
fn test_equality_alias_parsing() {
    // Tests that '=' is accepted as '=='
    let input = "A(d) : d=0 -> B";
    let (_, rule) = parse_rule(input).expect("Should parse d=0 as Eq");
    if let Some(Expr::Eq(lhs, rhs)) = rule.condition {
        assert!(matches!(*lhs, Expr::Variable(_)));
        assert!(matches!(*rhs, Expr::Number(_)));
    } else {
        panic!("Expected Expr::Eq");
    }
}

#[test]
fn test_explicit_equality_parsing() {
    // Regression check: '==' should still work
    let input = "A(d) : d==0 -> B";
    let (_, rule) = parse_rule(input).expect("Should parse d==0 as Eq");
    assert!(matches!(rule.condition, Some(Expr::Eq(_, _))));
}
