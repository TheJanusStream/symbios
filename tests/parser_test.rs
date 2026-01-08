use symbios::parser::{parse_module, parse_rule};

#[test]
fn test_parse_simple_module() {
    let (_, module) = parse_module("A(1.5, x)").unwrap();
    assert_eq!(module.symbol, 'A');
    assert_eq!(module.params.len(), 2);
    // Deep check would verify Expr::Number(1.5) and Expr::Variable("x")
}

#[test]
fn test_parse_context_rule() {
    // Rule: A(x) < B(y) > C : y > 5 -> B(y+1) A(x)
    let input = "A(x) < B(y) > C : y > 5 -> B(y+1) A(x)";
    let (_, rule) = parse_rule(input).unwrap();

    // Check Predecessor
    assert_eq!(rule.predecessor.symbol, 'B');
    assert_eq!(rule.predecessor.params.len(), 1); // y

    // Check Context
    assert_eq!(rule.left_context.unwrap().symbol, 'A');
    assert_eq!(rule.right_context.unwrap().symbol, 'C');

    // Check Condition
    assert!(rule.condition.is_some());

    // Check Successors
    assert_eq!(rule.successors.len(), 2);
    assert_eq!(rule.successors[0].symbol, 'B');
    assert_eq!(rule.successors[1].symbol, 'A');
}
