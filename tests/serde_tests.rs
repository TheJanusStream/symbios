use symbios::{SymbiosState, SymbolTable};

#[test]
fn test_interner_serde() {
    let mut table = SymbolTable::new();
    table.get_or_intern("A").unwrap();
    table.get_or_intern("B").unwrap();

    let serialized = serde_json::to_string(&table).unwrap();
    let deserialized: SymbolTable = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.resolve(0), Some("A"));
    assert_eq!(deserialized.resolve(1), Some("B"));
}

#[test]
fn test_state_serde() {
    let mut state = SymbiosState::new();

    state.current_time = 10.0;
    state.push(1, 0.5, &[10.0, 20.0]).unwrap();

    let serialized = serde_json::to_string(&state).unwrap();
    let deserialized: SymbiosState = serde_json::from_str(&serialized).unwrap();

    assert_eq!(deserialized.current_time, 10.0);
    let view = deserialized.get_view(0).unwrap();
    assert_eq!(view.sym, 1);
    assert_eq!(view.params, &[10.0, 20.0]);

    // Age should now correctly reconstruct as 0.5
    assert!(
        (view.age - 0.5).abs() < 1e-6,
        "Age mismatch: expected 0.5, got {}",
        view.age
    );
}
