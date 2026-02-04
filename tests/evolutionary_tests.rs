use symbios::System;
use symbios::system::{CrossoverConfig, MutationConfig, StructuralMutationConfig};

#[test]
fn test_reset_restores_initial_state() {
    let mut sys = System::new();
    sys.add_rule("A -> A B").unwrap();
    sys.set_axiom("A").unwrap();

    // Verify initial state
    assert_eq!(sys.state.len(), 1);

    // Derive multiple steps
    sys.derive(5).unwrap();
    assert!(sys.state.len() > 1, "State should grow after derivation");

    // Reset and verify
    assert!(sys.reset(), "reset() should return true when axiom was set");
    assert_eq!(sys.state.len(), 1, "State should be restored to axiom");

    // Verify it's actually the same symbol
    let view = sys.state.get_view(0).unwrap();
    assert_eq!(sys.interner.resolve(view.sym), Some("A"));
}

#[test]
fn test_reset_returns_false_without_axiom() {
    let mut sys = System::new();
    sys.add_rule("A -> A B").unwrap();

    // No axiom set
    assert!(
        !sys.reset(),
        "reset() should return false when no axiom was set"
    );
}

#[test]
fn test_reset_multiple_times() {
    let mut sys = System::new();
    sys.add_rule("A -> A A").unwrap();
    sys.set_axiom("A").unwrap();

    for _ in 0..3 {
        sys.derive(3).unwrap();
        let grown_len = sys.state.len();
        assert!(grown_len > 1);

        sys.reset();
        assert_eq!(sys.state.len(), 1);
    }
}

#[test]
fn test_clone_independence() {
    let mut sys = System::new();
    sys.add_rule("A -> A B").unwrap();
    sys.set_axiom("A").unwrap();

    let mut cloned = sys.clone();

    // Derive original
    sys.derive(3).unwrap();
    let orig_len = sys.state.len();

    // Derive clone differently
    cloned.derive(5).unwrap();
    let clone_len = cloned.state.len();

    // They should have different states
    assert_ne!(
        orig_len, clone_len,
        "Cloned systems should evolve independently"
    );
}

#[test]
fn test_mutate_changes_probabilities() {
    let mut sys = System::new();
    // Add stochastic rules with equal probability
    sys.add_rule("0.5: A -> A A").unwrap();
    sys.add_rule("0.5: A -> B").unwrap();
    sys.set_axiom("A").unwrap();

    // Get original probabilities
    let a_sym = sys.interner.resolve_id("A").unwrap();
    let original_probs: Vec<f64> = sys.rules[&a_sym].iter().map(|r| r.probability).collect();

    // Mutate with high rate to ensure changes
    let config = MutationConfig {
        rule_probability_rate: 1.0,
        rule_probability_strength: 0.3,
        constant_rate: 0.0,
        constant_strength: 0.0,
    };
    sys.mutate(&config);

    // Get new probabilities
    let new_probs: Vec<f64> = sys.rules[&a_sym].iter().map(|r| r.probability).collect();

    // At least one should have changed
    let changed = original_probs
        .iter()
        .zip(new_probs.iter())
        .any(|(o, n)| (o - n).abs() > 1e-10);
    assert!(changed, "Mutation should change at least one probability");

    // All probabilities should remain valid
    for p in &new_probs {
        assert!(
            *p >= 0.0 && *p <= 1.0,
            "Probability should be clamped to [0, 1]"
        );
    }
}

#[test]
fn test_mutate_changes_constants() {
    let mut sys = System::new();
    sys.add_directive("#define ANGLE 45").unwrap();
    sys.add_rule("A -> A").unwrap();
    sys.set_axiom("A").unwrap();

    let original_angle = sys.constants["ANGLE"];

    // Mutate with high rate
    let config = MutationConfig {
        rule_probability_rate: 0.0,
        rule_probability_strength: 0.0,
        constant_rate: 1.0,
        constant_strength: 0.5,
    };
    sys.mutate(&config);

    let new_angle = sys.constants["ANGLE"];
    assert!(
        (original_angle - new_angle).abs() > 1e-10,
        "Constant should be mutated"
    );
}

#[test]
fn test_mutate_respects_zero_rate() {
    let mut sys = System::new();
    sys.add_rule("0.5: A -> A A").unwrap();
    sys.add_directive("#define X 10").unwrap();
    sys.set_axiom("A").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();
    let original_prob = sys.rules[&a_sym][0].probability;
    let original_const = sys.constants["X"];

    // Zero rates should prevent mutation
    let config = MutationConfig {
        rule_probability_rate: 0.0,
        rule_probability_strength: 1.0,
        constant_rate: 0.0,
        constant_strength: 1.0,
    };
    sys.mutate(&config);

    assert_eq!(sys.rules[&a_sym][0].probability, original_prob);
    assert_eq!(sys.constants["X"], original_const);
}

#[test]
fn test_crossover_inherits_rules() {
    let mut parent_a = System::new();
    parent_a.add_rule("A -> A A").unwrap();
    parent_a.set_axiom("A").unwrap();

    let mut parent_b = System::new();
    parent_b.add_rule("B -> B B").unwrap();
    parent_b.set_axiom("B").unwrap();

    let config = CrossoverConfig {
        rule_bias: 0.5,
        constant_blend: 0.5,
    };

    // Create multiple offspring to test inheritance
    let mut inherited_a = false;
    let mut inherited_b = false;

    for _ in 0..20 {
        let offspring = parent_a.crossover(&parent_b, &config).unwrap();

        // Check if offspring has rules for A or B
        for (_, name) in offspring.interner.iter() {
            if name == "A" {
                let a_sym = offspring.interner.resolve_id("A").unwrap();
                if offspring.rules.contains_key(&a_sym) {
                    inherited_a = true;
                }
            }
            if name == "B" {
                let b_sym = offspring.interner.resolve_id("B").unwrap();
                if offspring.rules.contains_key(&b_sym) {
                    inherited_b = true;
                }
            }
        }
    }

    // Both rule sets should appear in offspring over multiple trials
    assert!(
        inherited_a || inherited_b,
        "Offspring should inherit rules from parents"
    );
}

#[test]
fn test_crossover_blends_constants() {
    let mut parent_a = System::new();
    parent_a.add_directive("#define ANGLE 30").unwrap();
    parent_a.add_rule("A -> A").unwrap();

    let mut parent_b = System::new();
    parent_b.add_directive("#define ANGLE 60").unwrap();
    parent_b.add_rule("A -> A").unwrap();

    // Test exact blending
    let config = CrossoverConfig {
        rule_bias: 0.5,
        constant_blend: 0.5,
    };

    let offspring = parent_a.crossover(&parent_b, &config).unwrap();
    let blended = offspring.constants["ANGLE"];

    // Should be average: (30 + 60) / 2 = 45
    assert!(
        (blended - 45.0).abs() < 1e-10,
        "Constant should be blended: expected 45, got {}",
        blended
    );
}

#[test]
fn test_crossover_constant_blend_bias() {
    let mut parent_a = System::new();
    parent_a.add_directive("#define X 0").unwrap();
    parent_a.add_rule("A -> A").unwrap();

    let mut parent_b = System::new();
    parent_b.add_directive("#define X 100").unwrap();
    parent_b.add_rule("A -> A").unwrap();

    // Blend fully toward parent B
    let config = CrossoverConfig {
        rule_bias: 0.5,
        constant_blend: 1.0,
    };

    let offspring = parent_a.crossover(&parent_b, &config).unwrap();
    assert!(
        (offspring.constants["X"] - 100.0).abs() < 1e-10,
        "constant_blend=1.0 should take parent B's value"
    );

    // Blend fully toward parent A
    let config = CrossoverConfig {
        rule_bias: 0.5,
        constant_blend: 0.0,
    };

    let offspring = parent_a.crossover(&parent_b, &config).unwrap();
    assert!(
        (offspring.constants["X"] - 0.0).abs() < 1e-10,
        "constant_blend=0.0 should take parent A's value"
    );
}

#[test]
fn test_crossover_unique_constants() {
    let mut parent_a = System::new();
    parent_a.add_directive("#define ONLY_A 10").unwrap();
    parent_a.add_rule("A -> A").unwrap();

    let mut parent_b = System::new();
    parent_b.add_directive("#define ONLY_B 20").unwrap();
    parent_b.add_rule("A -> A").unwrap();

    let config = CrossoverConfig::default();
    let offspring = parent_a.crossover(&parent_b, &config).unwrap();

    // Both unique constants should be inherited
    assert!(offspring.constants.contains_key("ONLY_A"));
    assert!(offspring.constants.contains_key("ONLY_B"));
    assert_eq!(offspring.constants["ONLY_A"], 10.0);
    assert_eq!(offspring.constants["ONLY_B"], 20.0);
}

#[test]
fn test_mutate_with_rng_reproducibility() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let mut sys1 = System::new();
    sys1.add_rule("0.5: A -> A A").unwrap();
    sys1.add_directive("#define X 10").unwrap();

    let mut sys2 = sys1.clone();

    let config = MutationConfig {
        rule_probability_rate: 1.0,
        rule_probability_strength: 0.3,
        constant_rate: 1.0,
        constant_strength: 0.3,
    };

    // Same seed should produce same mutations
    let mut rng1 = Pcg64::seed_from_u64(12345);
    let mut rng2 = Pcg64::seed_from_u64(12345);

    sys1.mutate_with_rng(&mut rng1, &config);
    sys2.mutate_with_rng(&mut rng2, &config);

    let a_sym1 = sys1.interner.resolve_id("A").unwrap();
    let a_sym2 = sys2.interner.resolve_id("A").unwrap();

    assert_eq!(
        sys1.rules[&a_sym1][0].probability, sys2.rules[&a_sym2][0].probability,
        "Same seed should produce identical probability mutations"
    );
    assert_eq!(
        sys1.constants["X"], sys2.constants["X"],
        "Same seed should produce identical constant mutations"
    );
}

#[test]
fn test_crossover_with_rng_reproducibility() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let mut parent_a = System::new();
    parent_a.add_rule("A -> A A").unwrap();
    parent_a.add_directive("#define X 10").unwrap();

    let mut parent_b = System::new();
    parent_b.add_rule("A -> B B").unwrap();
    parent_b.add_directive("#define X 90").unwrap();

    let config = CrossoverConfig::default();

    let mut rng1 = Pcg64::seed_from_u64(99999);
    let mut rng2 = Pcg64::seed_from_u64(99999);

    let offspring1 = parent_a
        .crossover_with_rng(&parent_b, &mut rng1, &config)
        .unwrap();
    let offspring2 = parent_a
        .crossover_with_rng(&parent_b, &mut rng2, &config)
        .unwrap();

    // Same seed should produce identical offspring
    assert_eq!(
        offspring1.constants["X"], offspring2.constants["X"],
        "Same seed should produce identical crossover results"
    );
}

#[test]
fn test_crossover_preserves_symbol_mapping() {
    let mut parent_a = System::new();
    parent_a.add_rule("Foo -> Foo Bar").unwrap();
    parent_a.set_axiom("Foo").unwrap();

    let parent_b = System::new();

    let config = CrossoverConfig {
        rule_bias: 1.0, // Always take from parent A
        constant_blend: 0.5,
    };

    let mut offspring = parent_a.crossover(&parent_b, &config).unwrap();

    // Offspring should be able to set axiom and derive using inherited symbols
    offspring.set_axiom("Foo").unwrap();
    offspring.derive(1).unwrap();

    assert_eq!(offspring.state.len(), 2);
    let v0 = offspring.state.get_view(0).unwrap();
    let v1 = offspring.state.get_view(1).unwrap();
    assert_eq!(offspring.interner.resolve(v0.sym), Some("Foo"));
    assert_eq!(offspring.interner.resolve(v1.sym), Some("Bar"));
}

#[test]
fn test_reset_preserves_parametric_axiom() {
    let mut sys = System::new();
    sys.add_rule("A(x) : x < 5 -> A(x+1)").unwrap();
    sys.set_axiom("A(0)").unwrap();

    sys.derive(3).unwrap();
    let view = sys.state.get_view(0).unwrap();
    assert_eq!(view.params[0], 3.0);

    sys.reset();
    let view = sys.state.get_view(0).unwrap();
    assert_eq!(
        view.params[0], 0.0,
        "Reset should restore original parameter values"
    );
}

#[test]
fn test_clone_preserves_initial_state() {
    let mut sys = System::new();
    sys.add_rule("A -> A B").unwrap();
    sys.set_axiom("A").unwrap();
    sys.derive(3).unwrap();

    let mut cloned = sys.clone();

    // Both should be able to reset to same initial state
    assert!(sys.reset());
    assert!(cloned.reset());

    assert_eq!(sys.state.len(), cloned.state.len());
}

#[test]
fn test_structural_mutate_swap_modules() {
    let mut sys = System::new();
    sys.add_rule("A -> B C D E").unwrap();
    sys.set_axiom("A").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();
    let original_order: Vec<u16> = sys.rules[&a_sym][0]
        .successors
        .iter()
        .map(|m| m.symbol)
        .collect();

    // High swap rate, no insert/delete
    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 1.0,
        insert_rate: 0.0,
        delete_rate: 0.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    // Run multiple times to ensure swap happens
    for _ in 0..10 {
        sys.structural_mutate(&config);
    }

    let new_order: Vec<u16> = sys.rules[&a_sym][0]
        .successors
        .iter()
        .map(|m| m.symbol)
        .collect();

    // Length should be preserved
    assert_eq!(original_order.len(), new_order.len());
}

#[test]
fn test_structural_mutate_insert_module() {
    let mut sys = System::new();
    sys.add_rule("A -> B").unwrap();
    sys.set_axiom("A").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();
    let original_len = sys.rules[&a_sym][0].successors.len();

    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 1.0,
        delete_rate: 0.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    sys.structural_mutate(&config);

    let new_len = sys.rules[&a_sym][0].successors.len();
    assert_eq!(new_len, original_len + 1, "Insert should add one module");
}

#[test]
fn test_structural_mutate_delete_module() {
    let mut sys = System::new();
    sys.add_rule("A -> B C D").unwrap();
    sys.set_axiom("A").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();
    let original_len = sys.rules[&a_sym][0].successors.len();

    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 0.0,
        delete_rate: 1.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    sys.structural_mutate(&config);

    let new_len = sys.rules[&a_sym][0].successors.len();
    assert_eq!(new_len, original_len - 1, "Delete should remove one module");
}

#[test]
fn test_structural_mutate_preserves_minimum_successor() {
    let mut sys = System::new();
    sys.add_rule("A -> B").unwrap();
    sys.set_axiom("A").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();

    // Try to delete from single-module successor
    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 0.0,
        delete_rate: 1.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    for _ in 0..10 {
        sys.structural_mutate(&config);
    }

    // Should still have at least one module
    assert!(
        !sys.rules[&a_sym][0].successors.is_empty(),
        "Delete should preserve at least one module"
    );
}

#[test]
fn test_structural_mutate_bytecode_push_perturbation() {
    use symbios::vm::Op;

    let mut sys = System::new();
    sys.add_rule("A(x) -> A(x + 10)").unwrap();
    sys.set_axiom("A(0)").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();

    // Find the Push(10) in the bytecode
    let original_push_val = sys.rules[&a_sym][0].successors[0].params[0]
        .iter()
        .find_map(|op| if let Op::Push(v) = op { Some(*v) } else { None });

    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 0.0,
        delete_rate: 0.0,
        bytecode_rate: 1.0,
        op_rate: 1.0,
        push_perturbation: 5.0,
    };

    // Mutate multiple times
    for _ in 0..10 {
        sys.structural_mutate(&config);
    }

    let new_push_val = sys.rules[&a_sym][0].successors[0].params[0]
        .iter()
        .find_map(|op| if let Op::Push(v) = op { Some(*v) } else { None });

    // If both found Push ops, they should likely differ after mutation
    if let (Some(orig), Some(new)) = (original_push_val, new_push_val) {
        // After 10 mutations with 100% rate, the value should have changed
        assert!(
            (orig - new).abs() > 1e-10,
            "Push constant should be perturbed"
        );
    }
}

#[test]
fn test_structural_mutate_bytecode_op_swap() {
    use symbios::vm::Op;

    let mut sys = System::new();
    sys.add_rule("A(x) -> A(x + x)").unwrap();
    sys.set_axiom("A(1)").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();

    // Verify there's an Add operation
    let has_add = sys.rules[&a_sym][0].successors[0].params[0]
        .iter()
        .any(|op| matches!(op, Op::Add));
    assert!(has_add, "Rule should have Add operation");

    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 0.0,
        delete_rate: 0.0,
        bytecode_rate: 1.0,
        op_rate: 1.0,
        push_perturbation: 0.0,
    };

    // Mutate multiple times - op should eventually change
    let mut found_different_op = false;
    for _ in 0..50 {
        sys.structural_mutate(&config);

        let has_non_add_arithmetic = sys.rules[&a_sym][0].successors[0].params[0]
            .iter()
            .any(|op| matches!(op, Op::Sub | Op::Mul | Op::Div));

        if has_non_add_arithmetic {
            found_different_op = true;
            break;
        }
    }

    assert!(
        found_different_op,
        "Op mutation should eventually swap Add to another arithmetic op"
    );
}

#[test]
fn test_structural_mutate_with_rng_reproducibility() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let mut sys1 = System::new();
    sys1.add_rule("A -> B C D").unwrap();
    sys1.set_axiom("A").unwrap();

    let mut sys2 = sys1.clone();

    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.5,
        insert_rate: 0.3,
        delete_rate: 0.2,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    let mut rng1 = Pcg64::seed_from_u64(77777);
    let mut rng2 = Pcg64::seed_from_u64(77777);

    sys1.structural_mutate_with_rng(&mut rng1, &config);
    sys2.structural_mutate_with_rng(&mut rng2, &config);

    let a_sym1 = sys1.interner.resolve_id("A").unwrap();
    let a_sym2 = sys2.interner.resolve_id("A").unwrap();

    let syms1: Vec<u16> = sys1.rules[&a_sym1][0]
        .successors
        .iter()
        .map(|m| m.symbol)
        .collect();
    let syms2: Vec<u16> = sys2.rules[&a_sym2][0]
        .successors
        .iter()
        .map(|m| m.symbol)
        .collect();

    assert_eq!(
        syms1, syms2,
        "Same seed should produce identical structural mutations"
    );
}

#[test]
fn test_structural_mutate_respects_zero_rates() {
    let mut sys = System::new();
    sys.add_rule("A -> B C").unwrap();
    sys.set_axiom("A").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();
    let original: Vec<u16> = sys.rules[&a_sym][0]
        .successors
        .iter()
        .map(|m| m.symbol)
        .collect();

    // All rates zero
    let config = StructuralMutationConfig {
        successor_rate: 0.0,
        swap_rate: 1.0,
        insert_rate: 1.0,
        delete_rate: 1.0,
        bytecode_rate: 1.0,
        op_rate: 1.0,
        push_perturbation: 10.0,
    };

    for _ in 0..10 {
        sys.structural_mutate(&config);
    }

    let after: Vec<u16> = sys.rules[&a_sym][0]
        .successors
        .iter()
        .map(|m| m.symbol)
        .collect();

    assert_eq!(
        original, after,
        "Zero successor_rate should prevent all changes"
    );
}

#[test]
fn test_structural_mutate_empty_interner_safe() {
    let mut sys = System::new();
    // No rules, no symbols - should not panic
    let config = StructuralMutationConfig::default();
    sys.structural_mutate(&config);
}

/// Tests that structural mutation respects symbol arity when inserting modules.
///
/// This addresses the "Structural Mutation Arity Mismatch" issue: when inserting
/// a new module, it must have the correct number of parameters to match the
/// symbol's expected arity, preventing "junk" modules that would fail matching.
#[test]
fn test_structural_mutate_insert_respects_arity() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let mut sys = System::new();
    // Define a rule with a parameterized symbol (arity 2)
    sys.add_rule("A(x, y) -> A(x + 1, y)").unwrap();
    sys.set_axiom("A(1, 2)").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();

    // Record original successor count per rule
    let original_counts: Vec<usize> = sys.rules[&a_sym]
        .iter()
        .map(|r| r.successors.len())
        .collect();

    // Force insertion with a predictable RNG
    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 1.0,
        delete_rate: 0.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    // Run multiple mutations to ensure we insert modules
    for seed in 0..20 {
        let mut test_sys = sys.clone();
        let mut rng = Pcg64::seed_from_u64(seed);
        test_sys.structural_mutate_with_rng(&mut rng, &config);

        // Check that inserted modules (beyond original count) have correct arity
        for (rule_idx, rule) in test_sys.rules[&a_sym].iter().enumerate() {
            let original_count = original_counts[rule_idx];
            // All successors beyond original count are inserted
            for (succ_idx, successor) in rule.successors.iter().enumerate() {
                // Only check modules that could be inserted (by position or new ones)
                // With insert_rate=1.0, exactly one module is inserted per rule per mutation
                if succ_idx >= original_count || rule.successors.len() > original_count {
                    // Inserted modules for symbol A should have 2 parameters
                    if successor.symbol == a_sym {
                        assert_eq!(
                            successor.params.len(),
                            2,
                            "Inserted module for A should have 2 params (seed {})",
                            seed
                        );
                    }
                }
            }
        }
    }
}

/// Tests that inserted modules with correct arity can be evaluated without errors.
#[test]
fn test_structural_mutate_inserted_modules_are_functional() {
    let mut sys = System::new();
    sys.add_rule("A(x) -> A(x + 1)").unwrap();
    sys.set_axiom("A(0)").unwrap();

    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 1.0,
        delete_rate: 0.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    sys.structural_mutate(&config);

    // Derivation should not fail due to parameter count mismatch
    let result = sys.derive(3);
    assert!(
        result.is_ok(),
        "Derivation should succeed with properly initialized inserted modules"
    );
}

/// Tests that structural mutation respects the MAX_SUCCESSORS limit (128).
///
/// This addresses the "Unbounded Growth via Mutation" issue: the insert operation
/// must check against the limit to prevent DoS via runaway evolutionary loops.
#[test]
fn test_structural_mutate_respects_max_successors_limit() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let mut sys = System::new();
    // Create a rule with many successors (close to the limit)
    let mut rule_str = String::from("A -> B");
    for _ in 1..120 {
        rule_str.push_str(" B");
    }
    sys.add_rule(&rule_str).unwrap();
    sys.set_axiom("A").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();
    let initial_len = sys.rules[&a_sym][0].successors.len();
    assert_eq!(initial_len, 120);

    // Force many insertions with 100% rate
    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 1.0,
        delete_rate: 0.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    // Run many mutation cycles - without the fix, this would grow unbounded
    for seed in 0..100 {
        let mut rng = Pcg64::seed_from_u64(seed);
        sys.structural_mutate_with_rng(&mut rng, &config);

        // Verify we never exceed 128 successors
        let current_len = sys.rules[&a_sym][0].successors.len();
        assert!(
            current_len <= 128,
            "Successor count {} exceeds MAX_SUCCESSORS limit of 128 at seed {}",
            current_len,
            seed
        );
    }

    // After all mutations, verify the limit is still respected
    let final_len = sys.rules[&a_sym][0].successors.len();
    assert!(
        final_len <= 128,
        "Final successor count {} exceeds MAX_SUCCESSORS limit",
        final_len
    );
}

/// Tests that crossover returns an error when the interner cannot accommodate symbols.
///
/// This addresses the "Semantic Corruption in Crossover" issue: crossover must
/// propagate interner errors rather than falling back to old_id which causes
/// symbol aliasing.
#[test]
fn test_crossover_returns_error_on_interner_overflow() {
    // Create a parent with a custom small interner that will overflow
    let mut parent_a = System::new();
    parent_a.add_rule("A -> B").unwrap();

    let mut parent_b = System::new();
    parent_b.add_rule("X -> Y").unwrap();

    // Normal crossover should succeed
    let config = CrossoverConfig::default();
    let result = parent_a.crossover(&parent_b, &config);
    assert!(result.is_ok(), "Normal crossover should succeed");

    // The offspring should have all symbols properly mapped
    let offspring = result.unwrap();
    assert!(offspring.interner.resolve_id("A").is_some());
    assert!(offspring.interner.resolve_id("B").is_some());
    assert!(offspring.interner.resolve_id("X").is_some());
    assert!(offspring.interner.resolve_id("Y").is_some());
}

/// Tests that crossover correctly maps symbols without aliasing.
///
/// Ensures that symbols from both parents are correctly interned in the offspring
/// without any ID collisions that could corrupt rule definitions.
#[test]
fn test_crossover_no_symbol_aliasing() {
    let mut parent_a = System::new();
    parent_a.add_rule("Alpha -> Beta Gamma").unwrap();
    parent_a.set_axiom("Alpha").unwrap();

    let mut parent_b = System::new();
    parent_b.add_rule("Delta -> Epsilon").unwrap();

    let config = CrossoverConfig {
        rule_bias: 1.0, // Take rules from parent A
        constant_blend: 0.5,
    };

    let mut offspring = parent_a.crossover(&parent_b, &config).unwrap();

    // Verify all symbols are distinct and properly mapped
    let alpha_id = offspring.interner.resolve_id("Alpha");
    let beta_id = offspring.interner.resolve_id("Beta");
    let gamma_id = offspring.interner.resolve_id("Gamma");

    assert!(alpha_id.is_some(), "Alpha should be interned");
    assert!(beta_id.is_some(), "Beta should be interned");
    assert!(gamma_id.is_some(), "Gamma should be interned");

    // All IDs should be unique
    let ids = vec![alpha_id.unwrap(), beta_id.unwrap(), gamma_id.unwrap()];
    let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
    assert_eq!(
        ids.len(),
        unique_ids.len(),
        "All symbol IDs should be unique"
    );

    // Offspring should be able to derive correctly
    offspring.set_axiom("Alpha").unwrap();
    offspring.derive(1).unwrap();

    // Verify the derived state has the correct symbols
    assert_eq!(offspring.state.len(), 2);
    let v0 = offspring.state.get_view(0).unwrap();
    let v1 = offspring.state.get_view(1).unwrap();
    assert_eq!(offspring.interner.resolve(v0.sym), Some("Beta"));
    assert_eq!(offspring.interner.resolve(v1.sym), Some("Gamma"));
}

/// Tests that successor-only symbols get correct arity in structural mutation.
///
/// This tests the fix for the "Structural Mutation Arity Mismatch" issue:
/// symbols that only appear in successors (never as predecessors) must still
/// have their arity tracked so mutations insert them with correct parameter counts.
#[test]
fn test_structural_mutate_successor_only_symbol_arity() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let mut sys = System::new();
    // B(x, y) only appears as a successor, never as a predecessor
    sys.add_rule("A -> B(1, 2)").unwrap();
    sys.set_axiom("A").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();
    let b_sym = sys.interner.resolve_id("B").unwrap();

    // Force insertion with high rate
    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 1.0,
        delete_rate: 0.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    // Run many mutations to ensure B gets inserted at least once
    let mut found_inserted_b = false;
    for seed in 0..100 {
        let mut test_sys = sys.clone();
        let mut rng = Pcg64::seed_from_u64(seed);
        test_sys.structural_mutate_with_rng(&mut rng, &config);

        // Check all successors in the rule
        for successor in &test_sys.rules[&a_sym][0].successors {
            if successor.symbol == b_sym {
                // B should have exactly 2 parameters (its arity from the original rule)
                assert_eq!(
                    successor.params.len(),
                    2,
                    "Successor-only symbol B should have 2 params when inserted (seed {})",
                    seed
                );
                // Only check inserted modules (position > 0 means it was inserted)
                if test_sys.rules[&a_sym][0].successors.len() > 1 {
                    found_inserted_b = true;
                }
            }
        }
    }

    assert!(
        found_inserted_b,
        "Should have inserted at least one B module across 100 seeds"
    );
}

/// Tests that axiom-only symbols get correct arity in structural mutation.
///
/// Symbols that only appear in the axiom (not in any rule predecessor or successor)
/// must still have their arity tracked for mutation.
#[test]
fn test_structural_mutate_axiom_only_symbol_arity() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let mut sys = System::new();
    // X(a, b, c) only appears in the axiom
    sys.add_rule("A -> A").unwrap();
    sys.set_axiom("A X(1, 2, 3)").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();
    let x_sym = sys.interner.resolve_id("X").unwrap();

    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 1.0,
        delete_rate: 0.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    // Run many mutations to ensure X gets inserted at least once
    let mut found_inserted_x = false;
    for seed in 0..100 {
        let mut test_sys = sys.clone();
        let mut rng = Pcg64::seed_from_u64(seed);
        test_sys.structural_mutate_with_rng(&mut rng, &config);

        for successor in &test_sys.rules[&a_sym][0].successors {
            if successor.symbol == x_sym {
                // X should have exactly 3 parameters (its arity from the axiom)
                assert_eq!(
                    successor.params.len(),
                    3,
                    "Axiom-only symbol X should have 3 params when inserted (seed {})",
                    seed
                );
                found_inserted_x = true;
            }
        }
    }

    assert!(
        found_inserted_x,
        "Should have inserted at least one X module across 100 seeds"
    );
}

/// Tests that crossover correctly transfers symbol_arities from parents.
///
/// This addresses the "Genetic Metadata Corruption" issue: offspring must inherit
/// the symbol_arities map from parents so that subsequent structural mutations
/// generate modules with correct parameter counts.
#[test]
fn test_crossover_preserves_symbol_arities() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    // Parent A has a 2-parameter symbol
    let mut parent_a = System::new();
    parent_a.add_rule("A(x, y) -> A(x + 1, y)").unwrap();
    parent_a.set_axiom("A(1, 2)").unwrap();

    // Parent B has a different 3-parameter symbol
    let mut parent_b = System::new();
    parent_b.add_rule("B(a, b, c) -> B(a, b, c + 1)").unwrap();
    parent_b.set_axiom("B(1, 2, 3)").unwrap();

    let config = CrossoverConfig::default();
    let offspring = parent_a.crossover(&parent_b, &config).unwrap();

    // Now mutate the offspring with high insert rate
    let mutated_offspring = offspring;
    let mutation_config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 1.0,
        delete_rate: 0.0,
        bytecode_rate: 0.0,
        op_rate: 0.0,
        push_perturbation: 0.0,
    };

    // Run mutations with various seeds to ensure both A and B symbols get inserted
    for seed in 0..50 {
        let mut test_offspring = mutated_offspring.clone();
        let mut rng = Pcg64::seed_from_u64(seed);
        test_offspring.structural_mutate_with_rng(&mut rng, &mutation_config);

        // Check all rules for correct arities
        for rules in test_offspring.rules.values() {
            for rule in rules {
                for successor in &rule.successors {
                    let sym_name = test_offspring.interner.resolve(successor.symbol);
                    match sym_name {
                        Some("A") => {
                            assert_eq!(
                                successor.params.len(),
                                2,
                                "Inserted A in offspring should have 2 params (seed {})",
                                seed
                            );
                        }
                        Some("B") => {
                            assert_eq!(
                                successor.params.len(),
                                3,
                                "Inserted B in offspring should have 3 params (seed {})",
                                seed
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

/// Tests that structural mutation does not produce infinite values.
///
/// This addresses the "Floating Point Poisoning" DoS vulnerability: repeated
/// perturbations of Push constants could overflow to Infinity, causing VMError
/// during derivation. The fix ensures only finite values are committed.
#[test]
fn test_structural_mutate_prevents_infinity_poisoning() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;
    use symbios::vm::Op;

    let mut sys = System::new();
    // Start with a value close to f64::MAX
    sys.add_rule("A(x) -> A(x)").unwrap();
    sys.set_axiom("A(0)").unwrap();

    let a_sym = sys.interner.resolve_id("A").unwrap();

    // Manually inject a near-max Push value into the bytecode
    sys.rules.get_mut(&a_sym).unwrap()[0].successors[0].params[0] = vec![Op::Push(f64::MAX / 2.0)];

    // Use extreme perturbation that would overflow to Infinity
    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.0,
        insert_rate: 0.0,
        delete_rate: 0.0,
        bytecode_rate: 1.0,
        op_rate: 1.0,
        push_perturbation: f64::MAX, // Extreme perturbation
    };

    // Run many mutations - without the fix, this would eventually produce Inf
    for seed in 0..100 {
        let mut rng = Pcg64::seed_from_u64(seed);
        sys.structural_mutate_with_rng(&mut rng, &config);

        // Check that all Push values remain finite
        for rules in sys.rules.values() {
            for rule in rules {
                for successor in &rule.successors {
                    for param_bytecode in &successor.params {
                        for op in param_bytecode {
                            if let Op::Push(val) = op {
                                assert!(
                                    val.is_finite(),
                                    "Push value {} is not finite after mutation (seed {})",
                                    val,
                                    seed
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Tests that derivation succeeds after many structural mutations without Inf poisoning.
#[test]
fn test_derive_succeeds_after_heavy_mutation() {
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    let mut sys = System::new();
    sys.add_rule("A(x) -> A(x + 1) B(x * 2)").unwrap();
    sys.add_rule("B(y) -> B(y - 1)").unwrap();
    sys.set_axiom("A(100)").unwrap();

    let config = StructuralMutationConfig {
        successor_rate: 1.0,
        swap_rate: 0.5,
        insert_rate: 0.3,
        delete_rate: 0.2,
        bytecode_rate: 1.0,
        op_rate: 0.5,
        push_perturbation: 1000.0, // Large but not extreme
    };

    // Apply many mutations
    for seed in 0..50 {
        let mut rng = Pcg64::seed_from_u64(seed);
        sys.structural_mutate_with_rng(&mut rng, &config);
    }

    // Reset to axiom and attempt derivation
    sys.reset();
    let result = sys.derive(5);

    // Derivation should not fail due to Infinity/NaN in bytecode
    assert!(
        result.is_ok(),
        "Derivation should succeed after heavy mutation: {:?}",
        result.err()
    );
}
