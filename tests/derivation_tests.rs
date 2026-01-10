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
