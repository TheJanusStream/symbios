use std::collections::HashMap;
use symbios::parser::ast::Expr;
use symbios::vm::{Compiler, Op, VirtualMachine};

#[test]
fn test_compile_and_eval_arithmetic() {
    // Context: module A(x, y) where x is index 0, y is index 1
    let params = vec!["x".to_string(), "y".to_string()];
    let constants = HashMap::new(); // Empty map
    let mut compiler = Compiler::new(params, &constants);

    // Expr: x * 2 + y
    let expr = Expr::Add(
        Box::new(Expr::Mul(
            Box::new(Expr::Variable("x".to_string())),
            Box::new(Expr::Number(2.0)),
        )),
        Box::new(Expr::Variable("y".to_string())),
    );

    let code = compiler.compile(&expr).expect("Compilation failed");

    // Verify Bytecode Structure
    // RPN: Load(0), Push(2.0), Mul, Load(1), Add
    assert_eq!(code.len(), 5);
    matches!(code[0], Op::LoadParam(0));
    matches!(code[1], Op::Push(2.0));
    matches!(code[2], Op::Mul);

    // Execution
    let mut vm = VirtualMachine::new();
    let args = vec![10.0, 5.0]; // x=10, y=5 -> 10*2 + 5 = 25
    let result = vm.eval(&code, &args, 0.0).expect("Runtime error");

    assert_eq!(result, 25.0);
}

#[test]
fn test_logic_and_guards() {
    // Context: A(t) : t > 5
    let params = vec!["t".to_string()];
    let constants = HashMap::new();
    let mut compiler = Compiler::new(params, &constants);

    // Expr: t > 5
    let expr = Expr::Gt(
        Box::new(Expr::Variable("t".to_string())),
        Box::new(Expr::Number(5.0)),
    );

    let code = compiler.compile(&expr).unwrap();
    let mut vm = VirtualMachine::new();

    // False case
    assert_eq!(vm.eval(&code, &[3.0], 0.0).unwrap(), 0.0);
    // True case
    assert_eq!(vm.eval(&code, &[6.0], 0.0).unwrap(), 1.0);
}

#[test]
fn test_stack_underflow_protection() {
    let mut vm = VirtualMachine::new();
    // Try to add with empty stack
    let code = vec![Op::Add];
    let res = vm.eval(&code, &[], 0.0);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Stack underflow");
}
