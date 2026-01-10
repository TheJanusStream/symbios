/* tests/math_tests.rs */
use symbios::System;

#[test]
fn test_standard_math_library() {
    let mut sys = System::new();

    // A(x) -> B(sin(x), floor(x), max(x, 10))
    sys.add_rule("A(x) -> B(sin(x)) C(floor(x)) D(max(x, 10))")
        .unwrap();

    // Axiom: A(1.570796)  (Approx PI/2)
    // sin(PI/2) ~= 1.0
    // floor(1.57) = 1.0
    // max(1.57, 10) = 10.0
    sys.set_axiom("A(1.57079632679)").unwrap();

    sys.derive(1).unwrap();

    let v_b = sys.state.get_view(0).unwrap();
    assert!((v_b.params[0] - 1.0).abs() < 1e-8, "sin failed");

    let v_c = sys.state.get_view(1).unwrap();
    assert_eq!(v_c.params[0], 1.0, "floor failed");

    let v_d = sys.state.get_view(2).unwrap();
    assert_eq!(v_d.params[0], 10.0, "max failed");
}

#[test]
fn test_math_error_handling() {
    let mut sys = System::new();
    // Sqrt of negative number -> MathError
    sys.add_rule("A(x) -> B(sqrt(x))").unwrap();
    sys.set_axiom("A(-1)").unwrap();

    let res = sys.derive(1);
    assert!(res.is_err());
    // Error string format depends on SystemError display, likely "VM error: Mathematical error..."
}
