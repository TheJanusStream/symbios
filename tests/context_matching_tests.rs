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
    let b_id = sys.interner.resolve_id("B").expect("B not interned");
    let rule = sys.rules[&b_id][0].clone();

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

    // FIX: Lookup rule by symbol ID "P"
    let p_id = sys.interner.resolve_id("P").expect("P not interned");
    let rule = sys.rules.get(&p_id).unwrap()[0].clone();

    setup_state(&mut sys, "L(10) P(5) R(15)");
    let is_match = matching::matches(&sys.state, 1, &rule, &sys.ignored_symbols, &mut vm)
        .expect("Match execution failed");
    assert!(is_match);

    setup_state(&mut sys, "L(10) P(5) R(20)");
    let is_match_neg = matching::matches(&sys.state, 1, &rule, &sys.ignored_symbols, &mut vm)
        .expect("Match execution failed");
    assert!(!is_match_neg);
}

#[test]
fn test_branch_skipping_abop_compliance() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();

    sys.add_rule("A > B -> X").unwrap();
    let a_id = sys.interner.resolve_id("A").expect("A not interned");
    let rule = sys.rules.get(&a_id).unwrap()[0].clone();

    setup_state(&mut sys, "A [ I ] B");
    let is_match = matching::matches(&sys.state, 0, &rule, &sys.ignored_symbols, &mut vm).unwrap();
    assert!(is_match);
}

#[test]
fn test_nested_branch_skipping() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();

    sys.add_rule("A > B -> X").unwrap();
    let a_id = sys.interner.resolve_id("A").unwrap();
    let rule = sys.rules.get(&a_id).unwrap()[0].clone();

    setup_state(&mut sys, "A [ X [ Y ] Z ] B");
    let is_match = matching::matches(&sys.state, 0, &rule, &sys.ignored_symbols, &mut vm).unwrap();
    assert!(is_match);
}

#[test]
fn test_parameter_alignment_hazard() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();

    sys.add_rule("A(x) > B(y) : x < y -> X").unwrap();
    let a_id = sys.interner.resolve_id("A").unwrap();
    let rule = sys.rules.get(&a_id).unwrap()[0].clone();

    setup_state(&mut sys, "A(10) B(20)");
    let res = matching::matches(&sys.state, 0, &rule, &sys.ignored_symbols, &mut vm).unwrap();
    assert!(res);

    setup_state(&mut sys, "A(10, 5) B(20)");
    let res_hazard =
        matching::matches(&sys.state, 0, &rule, &sys.ignored_symbols, &mut vm).unwrap();
    assert!(!res_hazard);
}
