use symbios::System;

#[test]
fn test_1_stochastic_normalization() {
    let mut sys = System::new();
    // Rules probabilities sum to 0.2, not 1.0
    sys.add_rule("0.1 : A -> B").unwrap();
    sys.add_rule("0.1 : A -> C").unwrap();

    sys.set_seed(123);
    let _ = sys.set_axiom("A A A A A A A A A A").unwrap(); // 10 iterations
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    let b_count = output.matches('B').count();
    let c_count = output.matches('C').count();

    assert!(
        b_count > 0 && c_count > 0,
        "Normalization failed: B: {}, C: {}",
        b_count,
        c_count
    );
    assert_eq!(
        b_count + c_count,
        10,
        "System should always select a matching rule if available"
    );
}

#[test]
fn test_2_deep_branch_context_matching() {
    let mut sys = System::new();
    // Predecessor A looks for E in right context, skipping complex branches
    sys.add_rule("A > E -> Success").unwrap();

    // Axiom: A [ B ] [ C [ D ] ] E
    // Needs topology calculated to skip
    sys.set_axiom("A [ B ] [ C [ D ] ] E").unwrap();
    let open = sys.interner.resolve_id("[").unwrap();
    let close = sys.interner.resolve_id("]").unwrap();
    sys.state.calculate_topology(open, close).unwrap();

    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    assert!(
        output.starts_with("Success"),
        "Failed to skip deep branches to find E"
    );
}

#[test]
fn test_3_floating_point_precision_in_guards() {
    let mut sys = System::new();
    sys.add_directive("#define STEP 0.1").unwrap();
    sys.set_axiom("A(0.0)").unwrap();

    sys.add_rule("A(t) : t == 0.3 -> B").unwrap();
    sys.add_rule("A(t) : t < 0.3 -> A(t + STEP)").unwrap();

    // Step 1: A(0.0) -> A(0.1)
    // Step 2: A(0.1) -> A(0.2)
    // Step 3: A(0.2) -> A(0.3)
    // Step 4: A(0.3) -> B  <-- We need 4 steps to see the transformation
    sys.derive(4).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    assert_eq!(output, "B", "Precision error: t == 0.3 failed to match");
}

#[test]
fn test_4_constant_redefinition() {
    let mut sys = System::new();
    sys.add_directive("#define X 10").unwrap();
    sys.add_directive("#define X 20").unwrap(); // Redefine

    sys.add_rule("A -> B(X)").unwrap();
    sys.set_axiom("A").unwrap();
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    assert_eq!(
        output, "B(20.0000)",
        "Constant redefinition failed to use most recent value"
    );
}

#[test]
fn test_5_total_mass_safety_limit() {
    let mut sys = System::new();
    sys.max_capacity = 100;

    sys.add_rule("A -> A A").unwrap();
    sys.set_axiom("A").unwrap();

    let res = sys.derive(10);

    assert!(res.is_err(), "System should have caught CapacityOverflow");

    // Check for the specific wrapped variant
    match res {
        Err(symbios::system::SystemError::State(symbios::core::SymbiosError::CapacityOverflow)) => {
            ()
        }
        other => panic!(
            "Expected SystemError::State(CapacityOverflow), got {:?}",
            other
        ),
    }
}

#[test]
fn test_6_complex_math_nesting() {
    let mut sys = System::new();
    // A(x) -> B(sin(cos(sqrt(x^2))))
    sys.add_rule("A(x) -> B(sin(cos(sqrt(x^2))))").unwrap();
    sys.set_axiom("A(4.0)").unwrap();

    sys.derive(1).unwrap();

    // Calculation: sin(cos(sqrt(16))) = sin(cos(4))
    let expected = (4.0f64.cos()).sin();

    let view = sys.state.get_view(0).unwrap();
    assert!(
        (view.params[0] - expected).abs() < 1e-8,
        "Nested math evaluation failed"
    );
}

#[test]
fn test_7_ignore_with_named_modules() {
    let mut sys = System::new();
    sys.add_directive("#ignore : internode").unwrap();
    sys.add_rule("A > B -> Success").unwrap();

    sys.set_axiom("A internode B").unwrap();
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    assert!(output.starts_with("Success"), "Named module ignore failed");
}

#[test]
fn test_8_ignore_list_breaks_topology() {
    let mut sys = System::new();

    sys.add_directive("#ignore : [ ]").unwrap();

    // Rule: If C is preceded by A, transform into Success (S)
    sys.add_rule("A < C -> S").unwrap();
    sys.set_axiom("A [ B ] C").unwrap();

    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));

    assert_eq!(
        output, "A [ B ] S",
        "Logic Defect: Ignoring brackets bypassed the topology skip-link, causing 'C' to see 'B' instead of 'A'."
    );
}
