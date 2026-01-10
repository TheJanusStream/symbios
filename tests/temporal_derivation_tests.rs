use symbios::System;

#[test]
fn test_age_driven_growth() {
    let mut sys = System::new();

    // Rule: Cell 'A' divides into 'B' and 'C' only after it is 5.0 time units old.
    // A(x) : age >= 5.0 -> B(x) C(x)
    sys.add_rule("A(x) : age >= 5.0 -> B(x) C(x)").unwrap();

    // Axiom
    sys.set_axiom("A(10)").unwrap();

    // --- T = 0.0 ---
    // Age is 0.0. Condition (0.0 >= 5.0) is FALSE.
    // System should apply Identity rule (A replaces itself).
    sys.derive(1).unwrap();

    let v0 = sys.state.get_view(0).unwrap();
    assert_eq!(sys.interner.resolve(v0.sym), Some("A"));
    assert_eq!(v0.age, 0.0);

    // --- T = 4.0 ---
    // Advance time. Age becomes 4.0.
    sys.state.advance_time(4.0).unwrap();

    // Condition (4.0 >= 5.0) is FALSE.
    sys.derive(1).unwrap();

    let v0 = sys.state.get_view(0).unwrap();
    assert_eq!(sys.interner.resolve(v0.sym), Some("A"));
    assert_eq!(v0.age, 4.0); // Age should be preserved

    // --- T = 6.0 ---
    // Advance time by 2.0. Total time = 6.0. Age = 6.0.
    sys.state.advance_time(2.0).unwrap();

    // Condition (6.0 >= 5.0) is TRUE.
    // Rule fires: A(10) -> B(10) C(10)
    sys.derive(1).unwrap();

    assert_eq!(sys.state.len(), 2);

    let v0 = sys.state.get_view(0).unwrap();
    assert_eq!(sys.interner.resolve(v0.sym), Some("B"));
    assert_eq!(v0.params[0], 10.0);
    assert_eq!(v0.age, 0.0); // Newborn modules have age 0

    let v1 = sys.state.get_view(1).unwrap();
    assert_eq!(sys.interner.resolve(v1.sym), Some("C"));
    assert_eq!(v1.params[0], 10.0);
    assert_eq!(v1.age, 0.0);
}

#[test]
fn test_age_access_in_successor() {
    let mut sys = System::new();

    // Rule: Pass the parent's age to the child parameter
    // A(x) -> B(age)
    sys.add_rule("A(x) -> B(age)").unwrap();

    sys.set_axiom("A(0)").unwrap();

    // Advance time to 10.5
    sys.state.advance_time(10.5).unwrap();

    // Derive
    sys.derive(1).unwrap();

    let v0 = sys.state.get_view(0).unwrap();
    assert_eq!(sys.interner.resolve(v0.sym), Some("B"));
    assert_eq!(v0.params[0], 10.5); // Parameter should be the parent's age
}
