use symbios::core::{SymbiosState, interner::SymbolTable};
use symbios::parser::parse_module;

#[test]
fn test_parser_interner_state_bridge() {
    // 1. Setup the "System" components
    let mut interner = SymbolTable::new();
    let mut state = SymbiosState::new();

    // 2. Parse a user string (Simulator Input)
    // "A(1, 2.5)"
    let input = "A(1, 2.5)";
    let (_, module_ast) = parse_module(input).expect("Failed to parse");

    // 3. Bridging Logic: String -> u16
    let sym_id = interner.intern(&module_ast.symbol);

    // Evaluate parameters (Simplified: assume they are numbers for this test)
    let evaluated_params: Vec<f64> = module_ast
        .params
        .iter()
        .map(|e| {
            match e {
                symbios::parser::ast::Expr::Number(n) => *n,
                _ => 0.0, // Mock eval
            }
        })
        .collect();

    // 4. Inject into Runtime State
    state
        .push(sym_id, &evaluated_params)
        .expect("Failed to push to state");

    // 5. Verify Integrety
    assert_eq!(sym_id, 0);
    assert_eq!(interner.resolve(0), Some("A"));

    // Check State View
    let view = state.get_view(0).expect("Should have module at 0");
    assert_eq!(view.sym, 0); // The ID, not "A"
    assert_eq!(view.params, &[1.0, 2.5]);
}
