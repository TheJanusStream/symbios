use symbios::System;

#[test]
fn test_continuous_time_advancement() {
    let mut system = System::new();
    // Axiom: A(10)
    system.set_axiom("A(10)").unwrap();

    // Initial state
    assert_eq!(system.state.get_view(0).unwrap().age, 0.0);

    // Advance time by 0.5 units
    system.state.advance_time(0.5).unwrap();

    let view = system.state.get_view(0).unwrap();
    assert_eq!(view.age, 0.5);
    assert_eq!(view.params[0], 10.0); // Parameters remain static
}
