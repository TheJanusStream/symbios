use symbios::core::interner::SymbolTable;

#[test]
fn test_intern_and_resolve() {
    let mut table = SymbolTable::new();

    let id_a = table.intern("A").unwrap();
    let id_b = table.intern("Branch").unwrap();
    let id_c = table.intern("+").unwrap();

    assert_eq!(id_a, 0);
    assert_eq!(id_b, 1);
    assert_eq!(id_c, 2);

    assert_eq!(table.resolve(0), Some("A"));
    assert_eq!(table.resolve(1), Some("Branch"));
    assert_eq!(table.resolve(2), Some("+"));
}

#[test]
fn test_deduplication() {
    let mut table = SymbolTable::new();

    let id_1 = table.intern("X").unwrap();
    let id_2 = table.intern("Y").unwrap();
    let id_1_again = table.intern("X").unwrap();

    assert_eq!(id_1, 0);
    assert_eq!(id_2, 1);
    assert_eq!(id_1_again, 0);
    assert_eq!(table.len(), 2);
}
