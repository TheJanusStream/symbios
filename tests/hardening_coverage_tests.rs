use symbios::System;
use symbios::system::matching::{self, MatchScratch};
use symbios::vm::{Op, VirtualMachine};

#[test]
fn test_vm_param_bounds_check() {
    let mut vm = VirtualMachine::new();
    let code = vec![Op::LoadParam(5)];
    let params = vec![1.0];
    let res = vm.eval(&code, &params, 0.0);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Parameter index out of bounds"));
}

#[test]
fn test_temporal_growth_logic() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    let mut scratch = MatchScratch::new();
    sys.add_rule("A : age > 5.0 -> B").unwrap();

    let a_id = sys.interner.resolve_id("A").unwrap();
    let rule = sys.rules[&a_id][0].clone();

    sys.set_axiom("A").unwrap();
    sys.state.current_time = 2.0;
    let match_early =
        matching::matches(&sys.state, 0, &rule, &[], &mut vm, &mut scratch).expect("Match failed");
    assert!(!match_early);

    sys.state.current_time = 6.0;
    let match_late =
        matching::matches(&sys.state, 0, &rule, &[], &mut vm, &mut scratch).expect("Match failed");
    assert!(match_late);
}

#[test]
fn test_neighbor_arity_mismatch() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    let mut scratch = MatchScratch::new();
    sys.add_rule("L(x) < P : x > 5 -> P").unwrap();

    // FIX: Resolve and access via HashMap
    let p_id = sys.interner.resolve_id("P").unwrap();
    let rule = sys.rules.get(&p_id).unwrap()[0].clone();

    sys.state.clear();
    let l = sys.interner.get_or_intern("L").unwrap();
    let p = sys.interner.get_or_intern("P").unwrap();
    sys.state.push(l, 0.0, &[10.0, 99.0]).unwrap();
    sys.state.push(p, 0.0, &[]).unwrap();
    sys.state.calculate_topology(100, 101).unwrap();

    let is_match = matching::matches(&sys.state, 1, &rule, &[], &mut vm, &mut scratch).unwrap();
    assert!(!is_match);
}

/// Test that advance_time prevents overflow to infinity (time-travel DoS fix).
/// If current_time becomes Inf, the system would be permanently bricked
/// because push() rejects non-finite birth_time values.
#[test]
fn test_advance_time_overflow_prevention() {
    use symbios::SymbiosState;

    let mut state = SymbiosState::new();
    // Start at a very large time
    state.current_time = f64::MAX / 2.0;

    // Small increment should succeed
    assert!(state.advance_time(1.0).is_ok());

    // Increment that would overflow to Inf should fail
    let result = state.advance_time(f64::MAX);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("overflow"));

    // State should still be usable (not bricked)
    assert!(state.current_time.is_finite());
    assert!(state.push(0, 0.0, &[]).is_ok());
}

/// Test that crossover produces functional offspring that can derive correctly.
/// This verifies that rule inheritance and symbol mapping work correctly.
#[test]
fn test_crossover_functional_offspring() {
    use symbios::system::CrossoverConfig;

    // Parent A: Fibonacci-like growth
    let mut parent_a = System::new();
    parent_a.add_rule("A -> A B").unwrap();
    parent_a.add_rule("B -> A").unwrap();
    parent_a.set_axiom("A").unwrap();

    // Parent B: Different growth pattern
    let mut parent_b = System::new();
    parent_b.add_rule("A -> B A").unwrap();
    parent_b.add_rule("B -> B").unwrap();
    parent_b.set_axiom("B").unwrap();

    // Create offspring with bias toward parent A
    let config = CrossoverConfig {
        rule_bias: 1.0,
        constant_blend: 0.5,
    };

    let mut offspring = parent_a.crossover(&parent_b, &config).unwrap();

    // Offspring should have A rules from parent_a
    let a_id = offspring.interner.resolve_id("A");
    assert!(a_id.is_some(), "Offspring should have A symbol");
    assert!(
        offspring.rules.contains_key(&a_id.unwrap()),
        "Offspring should have A rules"
    );

    // Set axiom and derive - should work without errors
    offspring.set_axiom("A").unwrap();
    offspring.derive(3).unwrap();

    // Verify derivation produced expected growth
    // A -> A B -> A B A -> A B A A B (Fibonacci: 1, 2, 3, 5)
    assert_eq!(
        offspring.state.len(),
        5,
        "Should have 5 symbols after 3 derivations"
    );
}

/// Test that derive loop correctly handles context-sensitive rules
/// after scratch buffer optimization (verifies hot-path allocation fix).
#[test]
fn test_derive_with_context_after_optimization() {
    let mut sys = System::new();

    // Context-sensitive rule: A < B > C -> D
    // This exercises the scratch buffer reuse optimization
    sys.add_rule("A < B > C -> D").unwrap();
    sys.add_rule("D -> D").unwrap(); // Identity to prevent disappearance
    sys.set_axiom("A B C").unwrap();

    sys.derive(1).unwrap();

    // After derivation: A D C (B matched with context and became D)
    assert_eq!(sys.state.len(), 3);

    let sym_0 = sys.state.get_view(0).unwrap().sym;
    let sym_1 = sys.state.get_view(1).unwrap().sym;
    let sym_2 = sys.state.get_view(2).unwrap().sym;

    let a_id = sys.interner.resolve_id("A").unwrap();
    let c_id = sys.interner.resolve_id("C").unwrap();
    let d_id = sys.interner.resolve_id("D").unwrap();

    assert_eq!(sym_0, a_id, "First symbol should be A");
    assert_eq!(
        sym_1, d_id,
        "Second symbol should be D (transformed from B)"
    );
    assert_eq!(sym_2, c_id, "Third symbol should be C");
}

/// Test multiple matching rules with scratch buffer reuse.
/// Verifies the double-matching optimization works correctly.
#[test]
fn test_derive_multiple_candidates_scratch_reuse() {
    let mut sys = System::new();
    sys.set_seed(42);

    // Multiple rules matching the same predecessor with context
    sys.add_rule("0.5: L(x) < A(y) -> B(x + y)").unwrap();
    sys.add_rule("0.5: L(x) < A(y) -> C(x * y)").unwrap();
    sys.set_axiom("L(2) A(3)").unwrap();

    // Run multiple derivations to exercise both code paths
    for _ in 0..10 {
        sys.reset();
        sys.derive(1).unwrap();

        // Verify derivation produced valid output
        assert_eq!(sys.state.len(), 2);

        let second = sys.state.get_view(1).unwrap();
        let b_id = sys.interner.resolve_id("B").unwrap();
        let c_id = sys.interner.resolve_id("C").unwrap();

        // Should be either B(5) or C(6)
        assert!(
            second.sym == b_id || second.sym == c_id,
            "Second symbol should be B or C"
        );

        if second.sym == b_id {
            assert_eq!(second.params[0], 5.0, "B should have param 2+3=5");
        } else {
            assert_eq!(second.params[0], 6.0, "C should have param 2*3=6");
        }
    }
}
