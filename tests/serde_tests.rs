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

#[test]
fn test_interner_deser_rejects_oversized_payload() {
    // Craft a JSON where total string bytes exceed max_bytes
    let payload = r#"{"to_str":["AAAAAAAAAA","BBBBBBBBBB"],"max_bytes":5}"#;
    let result: Result<SymbolTable, _> = serde_json::from_str(payload);
    assert!(
        result.is_err(),
        "Deserialization should reject payload exceeding max_bytes"
    );
}

#[test]
fn test_interner_deser_valid_payload() {
    // A well-formed payload within limits should succeed
    let payload = r#"{"to_str":["A","B","C"],"max_bytes":1000}"#;
    let table: SymbolTable = serde_json::from_str(payload).unwrap();
    assert_eq!(table.len(), 3);
    assert_eq!(table.resolve(0), Some("A"));
    assert_eq!(table.resolve(2), Some("C"));
}
