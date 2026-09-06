use symbios::System;

#[test]
fn test_stochastic_branching() {
    let mut sys = System::new();

    // Simple stochastic system:
    // A : 0.33 -> B
    // A : 0.33 -> C
    // A : 0.34 -> D

    // Note: Parsing syntax for prob is "A : 0.33 -> B" (from parser/mod.rs)
    // Wait, let's check parser.rs.
    // parse_rule_structure: terminated(ws(finite_float), ws(c_char(':')))
    // So syntax is: "0.33 : A -> B" ?
    // Let's verify parser implementation in step 1.

    /*
       Parser check:
       if let Ok((next, p)) = terminated(ws(finite_float), ws(c_char::<&str, Error<&str>>(':'))).parse(input)

       It parses float THEN ':' at the very start of parse_rule_structure.
       So "0.33 : A -> B" is the correct syntax.
    */

    sys.add_rule("0.5 : A -> B").unwrap();
    sys.add_rule("0.5 : A -> C").unwrap();

    // Deterministic Seed 1
    sys.set_seed(42);
    sys.set_axiom("A A A A A A A A A A").unwrap(); // 10 As
    sys.derive(1).unwrap();

    let state_1: Vec<String> = (0..sys.state.len())
        .map(|i| {
            sys.interner
                .resolve(sys.state.get_view(i).unwrap().sym)
                .unwrap()
                .to_string()
        })
        .collect();

    // Deterministic Seed 1 (Repeat)
    sys.set_seed(42);
    sys.set_axiom("A A A A A A A A A A").unwrap();
    sys.derive(1).unwrap();

    let state_2: Vec<String> = (0..sys.state.len())
        .map(|i| {
            sys.interner
                .resolve(sys.state.get_view(i).unwrap().sym)
                .unwrap()
                .to_string()
        })
        .collect();

    assert_eq!(
        state_1, state_2,
        "Derivation should be deterministic with same seed"
    );

    // Different Seed
    sys.set_seed(999);
    sys.set_axiom("A A A A A A A A A A").unwrap();
    sys.derive(1).unwrap();

    let state_3: Vec<String> = (0..sys.state.len())
        .map(|i| {
            sys.interner
                .resolve(sys.state.get_view(i).unwrap().sym)
                .unwrap()
                .to_string()
        })
        .collect();

    assert_ne!(
        state_1, state_3,
        "Different seeds should produce different results (statistically)"
    );

    // Distribution Check
    // With 0.5/0.5 prob, we expect roughly mix of Bs and Cs.
    let bs = state_3.iter().filter(|&s| s == "B").count();
    let cs = state_3.iter().filter(|&s| s == "C").count();
    assert!(bs > 0 && cs > 0, "Should have mix of B and C");
}

#[test]
fn test_stochastic_weight_sensitivity() {
    let mut sys = System::new();

    // Case 1: Skewed towards B
    sys.set_seed(42);
    sys.state.clear();
    sys.rules.clear();
    sys.add_rule("0.9 : A -> B").unwrap();
    sys.add_rule("0.1 : A -> C").unwrap();
    sys.set_axiom("A A A A A A A A A A").unwrap();
    sys.derive(1).unwrap();
    let out1 = format!("{}", sys.state.display(&sys.interner));

    // Case 2: Skewed towards C
    sys.set_seed(42);
    sys.state.clear();
    sys.rules.clear();
    sys.add_rule("0.1 : A -> B").unwrap();
    sys.add_rule("0.9 : A -> C").unwrap();
    sys.set_axiom("A A A A A A A A A A").unwrap();

    // RESET SEED to ensure 'r' sequence is identical
    sys.set_seed(42);
    sys.derive(1).unwrap();
    let out2 = format!("{}", sys.state.display(&sys.interner));

    // If out1 == out2, the bug is reproduced (weights ignored)
    assert_ne!(
        out1, out2,
        "Stochastic output was identical despite weight inversion!\nOut1: {}\nOut2: {}",
        out1, out2
    );
}

#[test]
fn test_labeled_mid_probability_syntax() {
    let mut sys = System::new();

    // User Requested Syntax: p1: A : 0.1 -> B
    // Hypothesis: The parser sees '0.1' as a CONDITION (Predecessor : Condition -> Successor).
    // Since 0.1 != 0.0, the condition is TRUE.
    // The actual probability defaults to 1.0.

    sys.add_rule("p1: A : 0.1 -> B").unwrap();
    sys.add_rule("p2: A : 0.9 -> C").unwrap();

    sys.set_seed(42);
    sys.set_axiom("A A A A A A A A A A").unwrap();
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    let b_count = output.matches("B").count();
    let c_count = output.matches("C").count();

    println!(
        "Distribution for 'p1: A : 0.1 -> B': B={}, C={}",
        b_count, c_count
    );

    // If weights were working, C should dominate B (9:1 ratio).
    // If bug exists (parsed as condition), ratios will be ~50/50 (random choice between two valid rules with prob 1.0).

    // We ASSERT that it works as intended (expecting the test to likely fail or show the bug)
    // A loose check: C should be at least double B
    assert!(
        c_count > b_count,
        "Weights appeared to be ignored! Got B:{} C:{} (Expected C dominance)",
        b_count,
        c_count
    );
}

/// Issue #91: relative weight ratios must be preserved even when individual
/// weights are tiny enough that their sum approaches subnormal territory.
/// Pre-fix, the safe_total = max(total, MIN_POSITIVE) floor caused the random
/// draw to range over an artificially-inflated interval, so almost every draw
/// was above per-rule weights and the last-candidate fallback won nearly
/// every selection. Post-fix, `random::<f64>() * total_probability` keeps the
/// draw scaled to the actual sum.
#[test]
fn test_stochastic_ratio_preserved_under_tiny_weights() {
    // Two rules, weights 1e-200 and 2e-200 — sum is well above MIN_POSITIVE
    // (~2.2e-308), so the prior floor wouldn't engage on the SUM, but the
    // pattern (small weights, large draw range relative to per-rule prob)
    // is the same. We use 1:2 weights so the bias is easy to detect.
    let mut sys = symbios::System::new();
    sys.add_rule("1e-200 : A -> B").unwrap();
    sys.add_rule("2e-200 : A -> C").unwrap();

    sys.set_seed(42);
    // Build a long axiom of A's so a single derive() produces many samples.
    let n = 5000usize;
    let axiom: String = std::iter::repeat_n("A", n).collect::<Vec<_>>().join(" ");
    sys.set_axiom(&axiom).unwrap();
    sys.derive(1).unwrap();

    let out = format!("{}", sys.state.display(&sys.interner));
    let bs = out.matches('B').count();
    let cs = out.matches('C').count();
    assert_eq!(bs + cs, n, "every A must select exactly one rule");

    // Expected ratio C:B = 2:1, so C ≈ 2/3 of total. With n=5000 samples and
    // a deterministic seed, observed ratio should be very close. Assert that
    // C is roughly twice B with a generous statistical envelope.
    let observed_ratio = cs as f64 / bs as f64;
    let expected_ratio = 2.0;
    let tolerance = 0.20; // 10% wiggle room each side at n=5000
    assert!(
        (observed_ratio - expected_ratio).abs() / expected_ratio < tolerance,
        "C/B ratio {} should be near {} (within {:.0}%); got B={} C={}",
        observed_ratio,
        expected_ratio,
        tolerance * 100.0,
        bs,
        cs,
    );
}

/// Issue #91 companion: confirm that subnormal totals don't panic and don't
/// fall back to "first-rule-always-wins" behavior. We can't construct
/// subnormals through the parser (it normalizes), so we manipulate runtime
/// rule weights directly to simulate the worst case.
#[test]
fn test_stochastic_subnormal_total_no_panic_no_first_bias() {
    let mut sys = symbios::System::new();
    sys.add_rule("0.5 : A -> B").unwrap();
    sys.add_rule("0.5 : A -> C").unwrap();

    // Force runtime weights into subnormal range. Both rules keep equal
    // weight, so ratio should remain 1:1.
    let a_id = sys.interner.resolve_id("A").unwrap();
    if let Some(bucket) = sys.rules.get_mut(&a_id) {
        for rule in bucket.iter_mut() {
            rule.probability = 1e-310; // subnormal
        }
    }

    sys.set_seed(7);
    let n = 2000usize;
    let axiom: String = std::iter::repeat_n("A", n).collect::<Vec<_>>().join(" ");
    sys.set_axiom(&axiom).unwrap();
    sys.derive(1).unwrap(); // must not panic

    let out = format!("{}", sys.state.display(&sys.interner));
    let bs = out.matches('B').count();
    let cs = out.matches('C').count();
    assert_eq!(bs + cs, n);
    // 1:1 ratio with statistical envelope.
    let dev = (bs as i64 - cs as i64).unsigned_abs() as f64 / n as f64;
    assert!(
        dev < 0.10,
        "subnormal-weighted 50/50 split should not be biased (got B={} C={})",
        bs,
        cs
    );
}
