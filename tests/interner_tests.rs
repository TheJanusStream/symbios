use symbios::core::interner::SymbolTable;

#[test]
fn test_intern_and_resolve() {
    let mut table = SymbolTable::new();

    // Intern new symbols
    let id_a = table.intern("A");
    let id_b = table.intern("Branch");
    let id_c = table.intern("+");

    // IDs should be sequential u16s
    assert_eq!(id_a, 0);
    assert_eq!(id_b, 1);
    assert_eq!(id_c, 2);

    // Resolve back to string
    assert_eq!(table.resolve(0), Some("A"));
    assert_eq!(table.resolve(1), Some("Branch"));
    assert_eq!(table.resolve(2), Some("+"));
    assert_eq!(table.resolve(99), None);
}

#[test]
fn test_deduplication() {
    let mut table = SymbolTable::new();

    let id_1 = table.intern("X");
    let id_2 = table.intern("Y");
    let id_1_again = table.intern("X");

    assert_eq!(id_1, 0);
    assert_eq!(id_2, 1);
    // Should return existing ID, not increment
    assert_eq!(id_1_again, 0);
    assert_eq!(table.len(), 2);
}
