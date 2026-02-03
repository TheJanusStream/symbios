use symbios::System;
use symbios::system::{CrossoverConfig, MutationConfig};

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
        let offspring = parent_a.crossover(&parent_b, &config);

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

    let offspring = parent_a.crossover(&parent_b, &config);
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

    let offspring = parent_a.crossover(&parent_b, &config);
    assert!(
        (offspring.constants["X"] - 100.0).abs() < 1e-10,
        "constant_blend=1.0 should take parent B's value"
    );

    // Blend fully toward parent A
    let config = CrossoverConfig {
        rule_bias: 0.5,
        constant_blend: 0.0,
    };

    let offspring = parent_a.crossover(&parent_b, &config);
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
    let offspring = parent_a.crossover(&parent_b, &config);

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

    let offspring1 = parent_a.crossover_with_rng(&parent_b, &mut rng1, &config);
    let offspring2 = parent_a.crossover_with_rng(&parent_b, &mut rng2, &config);

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

    let mut offspring = parent_a.crossover(&parent_b, &config);

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
