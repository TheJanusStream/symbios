use symbios::{SymbiosState, SymbolTable, System, system::mutate::StructuralMutationConfig};

#[test]
fn test_l_system_string_export() {
    let mut interner = SymbolTable::new();
    let a = interner.intern("A").unwrap();
    let b = interner.intern("B").unwrap();

    let mut state = SymbiosState::new();
    state.push(a, 0.0, &[1.5]).unwrap(); // A(1.5)
    state.push(b, 0.0, &[]).unwrap(); // B

    // Test formatting
    let output = format!("{}", state.display(&interner));
    assert_eq!(output, "A(1.5000) B");
}

#[test]
fn test_string_export_unknown_symbol() {
    let interner = SymbolTable::new();
    let mut state = SymbiosState::new();

    // Symbol 999 not in interner
    state.push(999, 0.0, &[]).unwrap();

    let output = format!("{}", state.display(&interner));
    assert_eq!(output, "?");
}

#[test]
fn test_export_rules_simple() {
    let mut sys = System::new();
    sys.add_rule("A -> A B").unwrap();
    sys.add_rule("B -> A").unwrap();

    let exported = sys.export_rules();
    assert_eq!(exported.len(), 2);

    // Find the rules (order is not guaranteed due to HashMap)
    let a_rule = exported.iter().find(|(p, _)| p == "A").unwrap();
    let b_rule = exported.iter().find(|(p, _)| p == "B").unwrap();

    assert_eq!(a_rule.1, "A -> A B");
    assert_eq!(b_rule.1, "B -> A");
}

#[test]
fn test_export_rules_parametric() {
    let mut sys = System::new();
    sys.add_rule("A(x) : x > 10 -> B(x + 1)").unwrap();

    let exported = sys.export_rules_for("A");
    assert_eq!(exported.len(), 1);
    // Preserves original param names from source
    assert_eq!(exported[0], "A(x) : x > 10 -> B(x + 1)");
}

#[test]
fn test_export_rule_with_custom_params() {
    let mut sys = System::new();
    sys.add_rule("A(x) : x > 10 -> B(x + 1)").unwrap();

    let exported = sys
        .export_rule_with_params("A", 0, vec!["x".into()])
        .unwrap();
    assert_eq!(exported, "A(x) : x > 10 -> B(x + 1)");
}

#[test]
fn test_export_rule_with_context() {
    let mut sys = System::new();
    sys.add_rule("A < B > C -> D").unwrap();

    let exported = sys.export_rules_for("B");
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0], "A < B > C -> D");
}

#[test]
fn test_export_stochastic_rule() {
    let mut sys = System::new();
    // Correct syntax: probability BEFORE the predecessor
    sys.add_rule("0.5 : A -> B").unwrap();

    let exported = sys.export_rules_for("A");
    assert_eq!(exported.len(), 1);
    assert_eq!(exported[0], "0.5 : A -> B");
}

#[test]
fn test_export_after_mutation() {
    let mut sys = System::new();
    sys.add_rule("A -> A B").unwrap();
    sys.set_seed(12345);

    // Mutate the system
    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        insert_rate: 0.0,
        delete_rate: 0.0,
        swap_rate: 1.0, // Force swap
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };
    sys.structural_mutate(&config);

    // Export the mutated rule
    let exported = sys.export_rules_for("A");
    assert_eq!(exported.len(), 1);
    // After swap, should be "A -> B A"
    assert_eq!(exported[0], "A -> B A");
}

#[test]
fn test_export_complex_expression() {
    let mut sys = System::new();
    sys.add_rule("A(x, y) -> B(x + y * 2, sin(x))").unwrap();

    let exported = sys
        .export_rule_with_params("A", 0, vec!["x".into(), "y".into()])
        .unwrap();
    assert_eq!(exported, "A(x, y) -> B(x + y * 2, sin(x))");
}

#[test]
fn test_export_rule_not_found() {
    let sys = System::new();
    let result = sys.export_rule_at("A", 0);
    assert!(result.is_err());
}

#[test]
fn test_round_trip_parse_export() {
    // This test demonstrates the full round-trip:
    // Source -> Parse -> Compile -> Export -> Source
    let mut sys = System::new();

    let original_rules = ["A -> A B", "B -> A"];

    for rule in &original_rules {
        sys.add_rule(rule).unwrap();
    }

    let exported = sys.export_rules();
    assert_eq!(exported.len(), 2);

    // Create a new system from exported rules
    let mut sys2 = System::new();
    for (_, rule_src) in &exported {
        sys2.add_rule(rule_src).unwrap();
    }

    // Verify they produce the same results
    sys.set_axiom("A").unwrap();
    sys2.set_axiom("A").unwrap();

    sys.derive(3).unwrap();
    sys2.derive(3).unwrap();

    let state1 = format!("{}", sys.state.display(&sys.interner));
    let state2 = format!("{}", sys2.state.display(&sys2.interner));

    assert_eq!(state1, state2);
}

#[test]
fn test_round_trip_stochastic_rules() {
    // Regression test: stochastic probabilities must survive export → re-import.
    let mut sys = System::new();
    sys.add_rule("0.3 : A -> B").unwrap();
    sys.add_rule("0.7 : A -> C").unwrap();
    sys.set_axiom("A A A A A A A A A A").unwrap();

    // Export and re-import
    let exported = sys.export_rules();
    let mut sys2 = System::new();
    for (_, rule_src) in &exported {
        sys2.add_rule(rule_src).unwrap();
    }
    sys2.set_axiom("A A A A A A A A A A").unwrap();

    // Both systems should produce identical output with same seed
    sys.set_seed(123);
    sys2.set_seed(123);
    sys.derive(1).unwrap();
    sys2.derive(1).unwrap();

    let state1 = format!("{}", sys.state.display(&sys.interner));
    let state2 = format!("{}", sys2.state.display(&sys2.interner));
    assert_eq!(
        state1, state2,
        "Stochastic round-trip failed: probabilities were lost during export/import"
    );
}

#[test]
fn test_round_trip_stochastic_from_source() {
    // End-to-end: from_source → to_source → from_source must preserve probabilities.
    let source = "omega: A A A A A\n0.4 : A -> B\n0.6 : A -> C";
    let sys1 = System::from_source(source).unwrap();
    let exported = sys1.to_source();
    let sys2 = System::from_source(&exported).unwrap();

    // Verify probabilities survived by comparing derivation outputs
    let mut s1 = sys1;
    let mut s2 = sys2;
    s1.set_seed(77);
    s2.set_seed(77);
    s1.derive(1).unwrap();
    s2.derive(1).unwrap();

    let out1 = format!("{}", s1.state.display(&s1.interner));
    let out2 = format!("{}", s2.state.display(&s2.interner));
    assert_eq!(
        out1, out2,
        "from_source round-trip lost stochastic probabilities.\nExported source:\n{}",
        exported
    );
}
