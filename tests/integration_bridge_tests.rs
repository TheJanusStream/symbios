use symbios::core::{SymbiosState, interner::SymbolTable};
use symbios::parser::parse_module;

#[test]
fn test_parser_interner_state_bridge() {
    let mut interner = SymbolTable::new();
    let mut state = SymbiosState::new();

    let input = "A(1, 2.5)";
    let (_, module_ast) = parse_module(input).expect("Failed to parse");

    let sym_id = interner.intern(&module_ast.symbol).expect("Intern failed");

    let evaluated_params: Vec<f64> = module_ast
        .params
        .iter()
        .map(|e| match e {
            symbios::parser::ast::Expr::Number(n) => *n,
            _ => 0.0,
        })
        .collect();

    state
        .push(sym_id, 0.0, &evaluated_params)
        .expect("Failed to push to state");

    assert_eq!(interner.resolve(sym_id), Some("A"));

    let view = state.get_view(0).expect("Should have module at 0");
    assert_eq!(view.sym, sym_id);
    assert_eq!(view.params, &[1.0, 2.5]);
}
