use symbios::System;

#[test]
fn test_derivation_anabaena() {
    // ABOP p. 17 (Eq 1.1) - Discrete Anabaena
    // ar -> al br
    // al -> bl ar
    // br -> ar
    // bl -> al

    let mut sys = System::new();
    sys.add_rule("ar -> al br").unwrap();
    sys.add_rule("al -> bl ar").unwrap();
    sys.add_rule("br -> ar").unwrap();
    sys.add_rule("bl -> al").unwrap();

    // Init: ar
    sys.set_axiom("ar").unwrap();

    // Step 1: al br
    sys.derive(1).unwrap();
    assert_eq!(sys.state.len(), 2);
    let s0 = sys
        .interner
        .resolve(sys.state.get_view(0).unwrap().sym)
        .unwrap();
    let s1 = sys
        .interner
        .resolve(sys.state.get_view(1).unwrap().sym)
        .unwrap();
    assert_eq!(s0, "al");
    assert_eq!(s1, "br");

    // Step 2: bl ar ar
    // al -> bl ar
    // br -> ar
    sys.derive(1).unwrap();
    assert_eq!(sys.state.len(), 3);
    let s0 = sys
        .interner
        .resolve(sys.state.get_view(0).unwrap().sym)
        .unwrap();
    let s1 = sys
        .interner
        .resolve(sys.state.get_view(1).unwrap().sym)
        .unwrap();
    let s2 = sys
        .interner
        .resolve(sys.state.get_view(2).unwrap().sym)
        .unwrap();

    assert_eq!(s0, "bl");
    assert_eq!(s1, "ar");
    assert_eq!(s2, "ar");
}

#[test]
fn test_derivation_parametric_abop_1_7() {
    // ABOP p. 42 (Eq 1.7) - Logic & Arithmetic
    // p1 : A(x, y) : y <= 3 -> A(x * 2, x + y)
    // p2 : A(x, y) : y > 3 -> B(x) A(x/y, 0)
    // p3 : B(x) : x < 1 -> C
    // p4 : B(x) : x >= 1 -> B(x - 1)

    let mut sys = System::new();
    sys.add_rule("A(x,y) : y <= 3 -> A(x*2, x+y)").unwrap();
    sys.add_rule("A(x,y) : y > 3 -> B(x) A(x/y, 0)").unwrap();
    sys.add_rule("B(x) : x < 1 -> C").unwrap();
    sys.add_rule("B(x) : x >= 1 -> B(x-1)").unwrap();

    // Axiom: B(2) A(4, 4)
    sys.set_axiom("B(2) A(4, 4)").unwrap();

    // Step 1
    // B(2) matches p4 (2 >= 1) -> B(1)
    // A(4, 4) matches p2 (4 > 3) -> B(4) A(1, 0)
    // Expected: B(1) B(4) A(1, 0)
    sys.derive(1).unwrap();

    assert_eq!(sys.state.len(), 3);

    // Check B(1)
    let v0 = sys.state.get_view(0).unwrap();
    assert_eq!(sys.interner.resolve(v0.sym), Some("B"));
    assert_eq!(v0.params[0], 1.0);

    // Check B(4)
    let v1 = sys.state.get_view(1).unwrap();
    assert_eq!(sys.interner.resolve(v1.sym), Some("B"));
    assert_eq!(v1.params[0], 4.0);

    // Check A(1, 0)
    let v2 = sys.state.get_view(2).unwrap();
    assert_eq!(sys.interner.resolve(v2.sym), Some("A"));
    assert_eq!(v2.params, &[1.0, 0.0]);

    // Step 2
    // B(1) -> B(0)
    // B(4) -> B(3)
    // A(1, 0) matches p1 (0 <= 3) -> A(2, 1)
    sys.derive(1).unwrap();

    assert_eq!(sys.state.len(), 3);
    let v2 = sys.state.get_view(2).unwrap();
    assert_eq!(v2.params, &[2.0, 1.0]);
}

/// Issue #93: `age` is a reserved identifier readable in both rule conditions
/// and successor parameter expressions. Verifies the ABOP §2.3 pattern of
/// passing the predecessor age into the next module.
#[test]
fn test_age_in_successor_expression() {
    let mut sys = System::new();
    // Identity rule that simply passes age into a parameter slot.
    // Predicate `age >= 0` keeps the rule live across all generations.
    sys.add_rule("A(x) : age >= 0 -> A(x + age)").unwrap();
    sys.set_axiom("A(0)").unwrap();

    // Walk the system in 1-step increments, advancing time between steps so
    // that the predecessor age grows. Each step's age is observed inside the
    // successor expression: x_{n+1} = x_n + age_n.
    sys.state.advance_time(1.0).unwrap();
    sys.derive(1).unwrap();
    // At step 1: age=1.0 (module's birth at t=0, current_time=1.0).
    // x becomes 0 + 1.0 = 1.0. Successor pushed with age 0.
    assert_eq!(sys.state.get_view(0).unwrap().params[0], 1.0);

    sys.state.advance_time(2.0).unwrap();
    sys.derive(1).unwrap();
    // age=2.0 (just-pushed module had age 0 at the start of derive).
    assert_eq!(sys.state.get_view(0).unwrap().params[0], 1.0 + 2.0);

    sys.state.advance_time(3.0).unwrap();
    sys.derive(1).unwrap();
    assert_eq!(sys.state.get_view(0).unwrap().params[0], 1.0 + 2.0 + 3.0);
}

/// Issue #93: `age` is reserved — it must be rejected as a parameter name in
/// any binding position (predecessor, left context, right context). Without
/// this check the compiler would silently emit `Op::LoadAge` for the
/// identifier `age`, ignoring the bound parameter.
#[test]
fn test_age_shadowing_is_rejected_in_predecessor() {
    let mut sys = System::new();
    let result = sys.add_rule("A(age) -> A(age + 1)");
    assert!(
        result.is_err(),
        "predecessor param named `age` must be rejected"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("age"), "error must mention `age`: {}", err);
    assert!(
        err.contains("reserved") || err.contains("predecessor"),
        "error must explain why: {}",
        err
    );
}

#[test]
fn test_age_shadowing_is_rejected_in_left_context() {
    let mut sys = System::new();
    let result = sys.add_rule("B(age) < A(x) -> A(x + 1)");
    assert!(
        result.is_err(),
        "left-context param named `age` must be rejected"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("age") && err.contains("left context"),
        "got: {}",
        err
    );
}

#[test]
fn test_age_shadowing_is_rejected_in_right_context() {
    let mut sys = System::new();
    let result = sys.add_rule("A(x) > B(age) -> A(x + 1)");
    assert!(
        result.is_err(),
        "right-context param named `age` must be rejected"
    );
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("age") && err.contains("right context"),
        "got: {}",
        err
    );
}

/// Issue #92: derivation single-rule fast path. A grammar where every
/// predecessor has exactly one rule must produce identical output with the
/// fast path engaged as it would with the slow path. We can't introspect
/// which path ran, but we can exercise a long derivation and assert it stays
/// internally consistent — and that probability=0 still suppresses the rule
/// (regression check for the fast path's probability filter).
#[test]
fn test_single_rule_fast_path_preserves_semantics() {
    let mut sys = System::new();
    // Every symbol has a single rule — fast path candidate.
    sys.add_rule("A -> B C").unwrap();
    sys.add_rule("B -> A").unwrap();
    sys.add_rule("C -> A B").unwrap();
    sys.set_axiom("A").unwrap();

    // 10 generations: state grows ~Fibonacci-fast. Expect deterministic length.
    sys.derive(10).unwrap();
    let len_after = sys.state.len();
    assert!(len_after > 100, "derivation should grow substantially");

    // Reset, re-derive, must match exactly.
    sys.reset();
    sys.derive(10).unwrap();
    assert_eq!(sys.state.len(), len_after);
}

#[test]
fn test_single_rule_fast_path_probability_zero_suppresses() {
    // A single rule with explicit probability 0 must be suppressed — same
    // semantics as the slow path. Without the probability filter in the
    // fast path, this rule would fire and turn A into B.
    let mut sys = System::new();
    sys.add_rule("0.0 : A -> B").unwrap();
    sys.set_axiom("A").unwrap();
    sys.derive(1).unwrap();
    let view = sys.state.get_view(0).unwrap();
    assert_eq!(
        sys.interner.resolve(view.sym),
        Some("A"),
        "p=0 single rule must not fire even via fast path"
    );
}
