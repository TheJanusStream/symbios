use symbios::{SymbiosState, SymbolTable};

#[test]
fn test_l_system_string_export() {
    let mut interner = SymbolTable::new();
    let a = interner.intern("A").unwrap();
    let b = interner.intern("B").unwrap();

    let mut state = SymbiosState::new();
    state.push(a, 0.0, &[1.5]).unwrap(); // A(1.5)
    state.push(b, 0.0, &[]).unwrap(); // B

    // Test formatting
    let output = format!("{}", state.display(&interner));
    assert_eq!(output, "A(1.5000) B");
}

#[test]
fn test_string_export_unknown_symbol() {
    let interner = SymbolTable::new();
    let mut state = SymbiosState::new();

    // Symbol 999 not in interner
    state.push(999, 0.0, &[]).unwrap();

    let output = format!("{}", state.display(&interner));
    assert_eq!(output, "?");
}
