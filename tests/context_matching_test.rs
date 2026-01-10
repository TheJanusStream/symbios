use symbios::System;
use symbios::system::matching;
use symbios::vm::VirtualMachine;

/// Helper to set up a system state for testing
fn setup_state(sys: &mut System, axiom: &str) {
    sys.set_axiom(axiom).expect("Failed to set axiom");

    // FIX: Handle Result from hardened interner
    let open = sys.interner.get_or_intern("[").expect("Intern failed");
    let close = sys.interner.get_or_intern("]").expect("Intern failed");

    sys.state
        .calculate_topology(open, close)
        .expect("Topology calc failed");
}

#[test]
fn test_stateless_context_1l_1r() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();

    sys.add_rule("A < B > C -> X").unwrap();

    // FIX: Clone rule to decouple from sys lifetime (E0502)
    let rule = sys.rules[0].clone();

    setup_state(&mut sys, "A B C");

    // FIX: Use new 5-arg signature taking &RuntimeRule
    let is_match = matching::matches(
        &sys.state,
        1, // Index of 'B'
        &rule,
        &sys.ignored_symbols,
        &mut vm,
    )
    .expect("Match execution failed");

    assert!(is_match, "B should match context A < B > C");
}

#[test]
fn test_parametric_context_aggregation() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();

    sys.add_rule("L(a) < P(b) > R(c) : a + b + c == 30 -> S")
        .unwrap();
    let rule = sys.rules[0].clone();

    // Positive Case
    setup_state(&mut sys, "L(10) P(5) R(15)");

    let is_match = matching::matches(
        &sys.state,
        1, // Index of P(5)
        &rule,
        &sys.ignored_symbols,
        &mut vm,
    )
    .expect("Match execution failed");

    assert!(is_match, "Should match: 10+5+15 == 30");

    // Negative Case
    setup_state(&mut sys, "L(10) P(5) R(20)"); // Sum = 35

    let is_match_neg = matching::matches(&sys.state, 1, &rule, &sys.ignored_symbols, &mut vm)
        .expect("Match execution failed");

    assert!(!is_match_neg, "Should fail: 10+5+20 != 30");
}

#[test]
fn test_branch_skipping_abop_compliance() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();

    // ABOP p.32: Signal propagation ignores branches not in the rule.
    sys.add_rule("A > B -> X").unwrap();
    let rule = sys.rules[0].clone();

    setup_state(&mut sys, "A [ I ] B");

    let is_match = matching::matches(
        &sys.state,
        0, // Index of A
        &rule,
        &sys.ignored_symbols,
        &mut vm,
    )
    .expect("Match execution failed");

    assert!(is_match, "Should skip branch [ I ] to find B");
}

#[test]
fn test_nested_branch_skipping() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();

    sys.add_rule("A > B -> X").unwrap();
    let rule = sys.rules[0].clone();

    setup_state(&mut sys, "A [ X [ Y ] Z ] B");

    let is_match = matching::matches(
        &sys.state,
        0, // A
        &rule,
        &sys.ignored_symbols,
        &mut vm,
    )
    .expect("Match execution failed");

    assert!(is_match, "Should skip nested branches to find B");
}

#[test]
fn test_parameter_alignment_hazard() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();

    // Rule expects arity: A(1), B(1).
    sys.add_rule("A(x) > B(y) : x < y -> X").unwrap();
    let rule = sys.rules[0].clone();

    // Case 1: Matching Arity (10 < 20) -> True
    setup_state(&mut sys, "A(10) B(20)");
    let res = matching::matches(&sys.state, 0, &rule, &sys.ignored_symbols, &mut vm).unwrap();
    assert!(res, "Aligned params should match");

    // Case 2: Misaligned Arity (Hazard)
    // A has 2 params (10, 5). Rule expects 1.
    // This MUST fail match to prevent the VM reading A(5) as B(y).
    setup_state(&mut sys, "A(10, 5) B(20)");
    let res_hazard =
        matching::matches(&sys.state, 0, &rule, &sys.ignored_symbols, &mut vm).unwrap();

    assert!(
        !res_hazard,
        "Arity mismatch should fail match immediately to prevent data corruption"
    );
}
