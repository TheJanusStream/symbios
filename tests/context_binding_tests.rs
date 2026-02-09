use symbios::System;

#[test]
fn test_context_parameter_binding_in_successors() {
    let mut sys = System::new();

    // Rule: Signal propagation where a child's value depends on its left neighbor
    // L(x) < P(y) -> S(x + y)
    sys.add_rule("L(x) < P(y) -> S(x + y)").unwrap();

    // Axiom: L(10) P(5)
    sys.set_axiom("L(10) P(5)").unwrap();

    // Step 1:
    // P(5) matches with Left Context L(10).
    // Successor should be S(10 + 5) = S(15.0)
    sys.derive(1)
        .expect("Derivation failed - likely due to missing context parameters");

    let output = format!("{}", sys.state.display(&sys.interner));
    // The L(10) remains (identity), P(5) becomes S(15)
    assert_eq!(output, "L(10.0000) S(15.0000)");
}

#[test]
fn test_complex_multi_context_binding() {
    let mut sys = System::new();

    // Rule with multiple modules in both left and right context
    // A(a) B(b) < P(p) > C(c) D(d) -> S(a + b + p + c + d)
    sys.add_rule("A(a) B(b) < P(p) > C(c) D(d) -> S(a + b + p + c + d)")
        .unwrap();

    // Axiom: A(1) B(2) P(10) C(3) D(4)
    sys.set_axiom("A(1) B(2) P(10) C(3) D(4)").unwrap();

    // Step 1: P(10) should see all neighbors.
    // Result should be 1+2+10+3+4 = 20
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    assert!(
        output.contains("S(20.0000)"),
        "Multi-neighbor binding failed. Output: {}",
        output
    );
}

#[test]
fn test_binding_with_ignore_list() {
    let mut sys = System::new();

    // Define a hormone/signal module that should be ignored for structural purposes
    // but skipped during context matching.
    sys.add_directive("#ignore : H").unwrap();

    // Rule: A sees B through the ignored H
    sys.add_rule("A(x) < B(y) -> S(x * y)").unwrap();

    // Axiom: A(10) H(999) B(5)
    sys.set_axiom("A(10) H(999) B(5)").unwrap();

    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    // If H(999) was incorrectly bound instead of A(10), we'd see 999 * 5.
    // If binding works correctly, we see 10 * 5.
    assert!(
        output.contains("S(50.0000)"),
        "Ignore-list binding error. Output: {}",
        output
    );
}

#[test]
fn test_binding_across_branches() {
    let mut sys = System::new();

    // Rule: Signal propagation ignores branches
    sys.add_rule("A(x) < B(y) -> S(x + y)").unwrap();

    // Axiom: A(10) [ C(99) ] B(5)
    sys.set_axiom("A(10) [ C(99) ] B(5)").unwrap();

    // Calculate topology so the skip_links are active
    let open = sys.interner.get_or_intern("[").unwrap();
    let close = sys.interner.get_or_intern("]").unwrap();
    sys.state.calculate_topology(open, close).unwrap();

    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    // Should be S(10 + 5)
    assert!(
        output.contains("S(15.0000)"),
        "Branch-skipping binding failed. Output: {}",
        output
    );
}

#[test]
fn test_multi_parameter_neighbor_binding() {
    let mut sys = System::new();

    // Neighbor has multiple params: L(x, y)
    sys.add_rule("L(x, y) < P(p) -> S(x, y, p)").unwrap();

    sys.set_axiom("L(1, 2) P(3)").unwrap();
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    assert!(
        output.contains("S(1.0000, 2.0000, 3.0000)"),
        "Multi-param neighbor binding failed."
    );
}

/// Regression test for scratch buffer reuse after a failed match clobbers it.
///
/// Scenario: Two rules match symbol P, both with left context. Rule 0 (with left
/// context L) matches. Rule 1 (with left context Z, which doesn't exist) is checked
/// next — `matches()` calls `scratch.clear()` even though Rule 1 fails.
/// If the derivation engine incorrectly reuses the now-empty scratch for Rule 0,
/// the left context parameters will be missing, producing wrong output.
#[test]
fn test_scratch_reuse_after_failed_match() {
    let mut sys = System::new();

    // Rule 0: L(x) < P(y) -> S(x + y)  — matches when L is the left neighbor
    // Rule 1: Z(z) < P(y) -> S(z * y)  — never matches (Z not in axiom)
    // Both rules target P with left context. Rule 1 is checked after Rule 0
    // and will clear scratch even though it fails.
    sys.add_rule("L(x) < P(y) -> S(x + y)").unwrap();
    sys.add_rule("Z(z) < P(y) -> S(z * y)").unwrap();

    sys.set_axiom("L(10) P(5)").unwrap();

    // Run multiple derivation steps to increase chance of hitting the bug
    // (single matching rule always selected, but scratch was clobbered by Rule 1 check)
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    // Correct: S(10 + 5) = S(15). If scratch was clobbered, left context params
    // would be missing, causing either a panic or S(5) (only predecessor param).
    assert_eq!(
        output, "L(10.0000) S(15.0000)",
        "Scratch buffer reuse corruption: left context params lost. Got: {}",
        output
    );
}

/// Same as above but with right context, ensuring both paths are protected.
#[test]
fn test_scratch_reuse_after_failed_match_right_context() {
    let mut sys = System::new();

    // Rule 0: P(y) > R(x) -> S(x + y)
    // Rule 1: P(y) > Z(z) -> S(z * y)  — never matches
    sys.add_rule("P(y) > R(x) -> S(x + y)").unwrap();
    sys.add_rule("P(y) > Z(z) -> S(z * y)").unwrap();

    sys.set_axiom("P(5) R(10)").unwrap();
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));
    assert_eq!(
        output, "S(15.0000) R(10.0000)",
        "Right-context scratch reuse corruption. Got: {}",
        output
    );
}
