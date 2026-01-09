use symbios::{SymbiosState, core::SymbiosError};

#[test]
fn test_soa_layout_integrity() {
    let mut state = SymbiosState::new();
    state.push(1, &[1.0, 2.0, 3.0]).unwrap();
    state.push(2, &[]).unwrap();
    state.push(3, &[4.0]).unwrap();

    let view_a = state.get_view(0).expect("Should have index 0");
    assert_eq!(view_a.params, &[1.0, 2.0, 3.0]);

    let view_c = state.get_view(2).expect("Should have index 2");
    assert_eq!(view_c.params, &[4.0]);
}

#[test]
fn test_topology_calculation() {
    let mut state = SymbiosState::new();
    let (open, close, leaf) = (100, 101, 1);

    state.push(leaf, &[]).unwrap(); // 0
    state.push(open, &[]).unwrap(); // 1
    state.push(leaf, &[]).unwrap(); // 2
    state.push(close, &[]).unwrap(); // 3

    state
        .calculate_topology(open, close)
        .expect("Should validate");

    assert_eq!(state.get_view(1).unwrap().skip_idx, Some(3));
    assert_eq!(state.get_view(0).unwrap().skip_idx, None);
}

#[test]
fn test_state_clearing() {
    let mut state = SymbiosState::new();
    state.push(1, &[1.0]).unwrap();
    state.clear();
    assert!(state.is_empty()); // This now compiles
}

#[test]
fn test_param_overflow_safeguard() {
    let mut state = SymbiosState::new();
    // 65536 is u16::MAX + 1
    let huge_params = vec![0.0; 65536];
    let res = state.push(1, &huge_params);

    // Explicitly verify the error type and values
    match res {
        Err(SymbiosError::ParameterOverflow(65536, 65535)) => (),
        _ => panic!("Expected ParameterOverflow(65536, 65535), got {:?}", res),
    }
}

#[test]
fn test_topology_errors() {
    let mut state = SymbiosState::new();
    let (open, close) = (100, 101);

    state.push(open, &[]).unwrap();
    assert_eq!(
        state.calculate_topology(open, close),
        Err(SymbiosError::UnmatchedBracket(0))
    );

    state.clear();
    state.push(close, &[]).unwrap();
    assert_eq!(
        state.calculate_topology(open, close),
        Err(SymbiosError::UnmatchedBracket(0))
    );
}
