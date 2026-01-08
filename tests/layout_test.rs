use symbios::SymbiosState;

#[test]
fn test_soa_push_and_view() {
    let mut state = SymbiosState::new();

    // Push Symbol 1 (A) with 2 params
    state.push(1, &[10.0, 20.0]);
    // Push Symbol 2 (B) with 0 params
    state.push(2, &[]);
    // Push Symbol 3 (C) with 3 params
    state.push(3, &[0.1, 0.2, 0.3]);

    // Verify Symbol A
    let view_a = state.get_view(0).unwrap();
    assert_eq!(view_a.sym, 1);
    assert_eq!(view_a.params, &[10.0, 20.0]);

    // Verify Symbol B
    let view_b = state.get_view(1).unwrap();
    assert_eq!(view_b.sym, 2);
    assert_eq!(view_b.params, &[]);

    // Verify Symbol C
    let view_c = state.get_view(2).unwrap();
    assert_eq!(view_c.sym, 3);
    assert_eq!(view_c.params, &[0.1, 0.2, 0.3]);

    // Verify Arena density
    assert_eq!(state.symbols.len(), 3);
    assert_eq!(state.params.len(), 5); // 2 + 0 + 3
}
