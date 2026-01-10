use symbios::System;

#[test]
fn test_define_constants() {
    let mut sys = System::new();

    // Define PI and verify calculation
    sys.add_directive("#define PI 3.14159").unwrap();
    sys.add_directive("#define DOUBLE_PI PI * 2").unwrap();

    assert!(sys.constants.contains_key("PI"));
    assert!(sys.constants.contains_key("DOUBLE_PI"));

    let double_pi = *sys.constants.get("DOUBLE_PI").unwrap();
    assert!((double_pi - 6.28318).abs() < 1e-4);
}

#[test]
fn test_constant_propagation_to_rules() {
    let mut sys = System::new();

    sys.add_directive("#define THRESHOLD 10").unwrap();
    // Rule uses constant in condition
    sys.add_rule("A(x) : x > THRESHOLD -> B").unwrap();

    // Test logic
    sys.set_axiom("A(5) A(15)").unwrap();
    sys.derive(1).unwrap();

    // A(5) should stay A (5 > 10 is false)
    let v0 = sys.state.get_view(0).unwrap();
    assert_eq!(sys.interner.resolve(v0.sym), Some("A"));

    // A(15) should become B (15 > 10 is true)
    let v1 = sys.state.get_view(1).unwrap();
    assert_eq!(sys.interner.resolve(v1.sym), Some("B"));
}

#[test]
fn test_ignore_directive() {
    let mut sys = System::new();

    // Ignored symbols in context
    sys.add_directive("#ignore : + -").unwrap();

    // Rule: A > B -> X (context sensitive)
    sys.add_rule("A > B -> X").unwrap();

    // Axiom: A + - B
    // If ignore works, A should see B
    sys.set_axiom("A + - B").unwrap();

    sys.derive(1).unwrap();

    let v0 = sys.state.get_view(0).unwrap();
    assert_eq!(
        sys.interner.resolve(v0.sym),
        Some("X"),
        "Ignored symbols should allow context match"
    );
}

#[test]
fn test_constant_in_axiom() {
    let mut sys = System::new();
    sys.add_directive("#define START 100").unwrap();

    sys.set_axiom("A(START)").unwrap();

    let v = sys.state.get_view(0).unwrap();
    assert_eq!(v.params[0], 100.0);
}
