use symbios::System;
use symbios::system::crossover::CrossoverConfig;
use symbios::system::matching::{self, MatchScratch};
use symbios::vm::{MathOp, Op, VirtualMachine};

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

/// Test that explicit probability prefix is preserved when condition is a number.
/// Bug: "0.9 : A : 1 -> B" was incorrectly overwriting probability 0.9 with condition 1.0.
#[test]
fn test_explicit_probability_not_overwritten_by_numeric_condition() {
    let mut sys = System::new();
    sys.set_seed(12345);

    // Rule with explicit probability 0.0 and numeric condition 1 (always true).
    // The probability should remain 0.0, meaning the rule should never fire.
    sys.add_rule("0.0 : A : 1 -> B").unwrap();
    sys.set_axiom("A").unwrap();

    // Run multiple derivations - rule should never fire (probability 0.0)
    for _ in 0..10 {
        sys.reset();
        sys.derive(1).unwrap();

        let sym = sys.state.get_view(0).unwrap().sym;
        let a_id = sys.interner.resolve_id("A").unwrap();
        assert_eq!(
            sym, a_id,
            "A should remain A (probability 0.0 rule should never fire)"
        );
    }
}

/// Test that syntactic sugar "A : 0.5 -> B" (condition-as-probability) still works.
/// Note: In this L-system, probability is a RELATIVE WEIGHT for selecting among
/// multiple matching rules. A single rule with weight 0.5 fires 100% of the time.
/// To test probabilistic selection, we need multiple competing rules.
#[test]
fn test_condition_as_probability_sugar() {
    let mut sys = System::new();
    sys.set_seed(42);

    // Syntactic sugar: condition acts as probability/weight when no explicit prefix given.
    // With two rules having weights 0.3 and 0.7, we expect ~30%/~70% selection.
    sys.add_rule("A : 0.3 -> B").unwrap();
    sys.add_rule("A : 0.7 -> C").unwrap();
    sys.set_axiom("A").unwrap();

    // Run many derivations and count outcomes
    let mut b_count = 0;
    let mut c_count = 0;
    for _ in 0..100 {
        sys.reset();
        sys.derive(1).unwrap();

        let sym = sys.state.get_view(0).unwrap().sym;
        let b_id = sys.interner.resolve_id("B").unwrap();
        let c_id = sys.interner.resolve_id("C").unwrap();
        if sym == b_id {
            b_count += 1;
        } else if sym == c_id {
            c_count += 1;
        }
    }

    // With weights 0.3 and 0.7, we expect roughly 30% B and 70% C
    assert!(
        b_count > 10 && b_count < 50,
        "Expected ~30% B's, got {} out of 100",
        b_count
    );
    assert!(
        c_count > 50 && c_count < 90,
        "Expected ~70% C's, got {} out of 100",
        c_count
    );
}

/// Test that MathOp::arity() returns correct values for all operations.
/// This ensures Op::Math cannot have inconsistent arity (API safety fix).
#[test]
fn test_math_op_arity_correctness() {
    // Unary operations should have arity 1
    assert_eq!(MathOp::Sin.arity(), 1);
    assert_eq!(MathOp::Cos.arity(), 1);
    assert_eq!(MathOp::Tan.arity(), 1);
    assert_eq!(MathOp::Sqrt.arity(), 1);
    assert_eq!(MathOp::Abs.arity(), 1);
    assert_eq!(MathOp::Floor.arity(), 1);
    assert_eq!(MathOp::Ceil.arity(), 1);
    assert_eq!(MathOp::Round.arity(), 1);

    // Binary operations should have arity 2
    assert_eq!(MathOp::Min.arity(), 2);
    assert_eq!(MathOp::Max.arity(), 2);
}

/// Test that manually constructed Op::Math uses correct arity from MathOp.
/// Previously, Op::Math(MathOp, u8) allowed mismatched arity values.
/// Now Op::Math(MathOp) derives arity from the MathOp itself.
#[test]
fn test_vm_math_op_stack_consistency() {
    let mut vm = VirtualMachine::new();

    // Test unary op (sin) with exactly 1 value on stack - should succeed
    let code = vec![Op::Push(1.0), Op::Math(MathOp::Sin)];
    let result = vm.eval(&code, &[], 0.0);
    assert!(result.is_ok(), "sin(1.0) should succeed");

    // Test binary op (max) with exactly 2 values on stack - should succeed
    let code = vec![Op::Push(1.0), Op::Push(2.0), Op::Math(MathOp::Max)];
    let result = vm.eval(&code, &[], 0.0);
    assert!(result.is_ok(), "max(1.0, 2.0) should succeed");
    assert_eq!(result.unwrap(), 2.0);

    // Test unary op with insufficient stack - should fail with underflow
    let code = vec![Op::Math(MathOp::Sin)];
    let result = vm.eval(&code, &[], 0.0);
    assert!(result.is_err(), "sin() with empty stack should fail");
    assert!(result.unwrap_err().contains("underflow"));

    // Test binary op with only 1 value - should fail with underflow
    let code = vec![Op::Push(1.0), Op::Math(MathOp::Max)];
    let result = vm.eval(&code, &[], 0.0);
    assert!(result.is_err(), "max() with 1 value should fail");
    assert!(result.unwrap_err().contains("underflow"));
}
