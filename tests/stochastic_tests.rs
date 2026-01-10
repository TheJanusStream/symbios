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
