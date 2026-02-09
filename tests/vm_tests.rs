use std::collections::HashMap;
use symbios::parser::ast::Expr;
use symbios::vm::{Compiler, MathOp, Op, VirtualMachine};

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

/// Tests that comparison operators maintain mathematical consistency with epsilon-based equality.
///
/// This addresses the "Comparison Consistency Violation" issue: if two numbers are
/// "equal" via float_eq (epsilon tolerance), then Ge and Le must also return true.
/// Mathematical transitivity requires: (a == b) implies (a >= b) and (a <= b).
#[test]
fn test_comparison_epsilon_consistency() {
    use symbios::vm::float_eq;

    let mut vm = VirtualMachine::new();

    // Two values that are "equal" via epsilon but not strictly equal
    let a = 1.0;
    let b = 1.0 + f64::EPSILON * 10.0; // Within epsilon tolerance

    // Verify they ARE considered equal by float_eq
    assert!(float_eq(a, b), "a and b should be epsilon-equal");

    // Test Eq returns true (should pass by design)
    let code_eq = vec![Op::Push(a), Op::Push(b), Op::Eq];
    assert_eq!(
        vm.eval(&code_eq, &[], 0.0).unwrap(),
        1.0,
        "Eq should return true for epsilon-equal values"
    );

    // Test Ge: if a == b (epsilon), then a >= b must be true
    let code_ge = vec![Op::Push(a), Op::Push(b), Op::Ge];
    assert_eq!(
        vm.eval(&code_ge, &[], 0.0).unwrap(),
        1.0,
        "Ge should return true when a is epsilon-equal to b"
    );

    // Test Le: if a == b (epsilon), then a <= b must be true
    let code_le = vec![Op::Push(a), Op::Push(b), Op::Le];
    assert_eq!(
        vm.eval(&code_le, &[], 0.0).unwrap(),
        1.0,
        "Le should return true when a is epsilon-equal to b"
    );

    // Test Gt: if a == b (epsilon), then a > b must be false
    let code_gt = vec![Op::Push(a), Op::Push(b), Op::Gt];
    assert_eq!(
        vm.eval(&code_gt, &[], 0.0).unwrap(),
        0.0,
        "Gt should return false when a is epsilon-equal to b"
    );

    // Test Lt: if a == b (epsilon), then a < b must be false
    let code_lt = vec![Op::Push(a), Op::Push(b), Op::Lt];
    assert_eq!(
        vm.eval(&code_lt, &[], 0.0).unwrap(),
        0.0,
        "Lt should return false when a is epsilon-equal to b"
    );

    // Test strict inequality still works for clearly different values
    let clearly_greater = 2.0;
    let clearly_less = 0.5;

    let code_gt_clear = vec![Op::Push(clearly_greater), Op::Push(a), Op::Gt];
    assert_eq!(
        vm.eval(&code_gt_clear, &[], 0.0).unwrap(),
        1.0,
        "Gt should return true for clearly greater values"
    );

    let code_lt_clear = vec![Op::Push(clearly_less), Op::Push(a), Op::Lt];
    assert_eq!(
        vm.eval(&code_lt_clear, &[], 0.0).unwrap(),
        1.0,
        "Lt should return true for clearly lesser values"
    );
}

#[test]
fn test_vm_rejects_infinity_in_arithmetic() {
    let mut vm = VirtualMachine::new();

    // Addition overflow to infinity
    let code = vec![Op::Push(f64::MAX), Op::Push(f64::MAX), Op::Add];
    let res = vm.eval(&code, &[], 0.0);
    assert!(res.is_err(), "Addition producing Inf should be rejected");

    // Division by zero produces Inf (not NaN, since div maps 0 -> NaN for divisor,
    // but large / tiny can produce Inf)
    let code = vec![Op::Push(1e308), Op::Push(1e-308), Op::Div];
    let res = vm.eval(&code, &[], 0.0);
    assert!(res.is_err(), "Division producing Inf should be rejected");

    // Pow overflow
    let code = vec![Op::Push(1e300), Op::Push(2.0), Op::Pow];
    let res = vm.eval(&code, &[], 0.0);
    assert!(res.is_err(), "Pow producing Inf should be rejected");

    // Tan at pi/2 (produces very large / Inf depending on precision)
    let code = vec![Op::Push(std::f64::consts::FRAC_PI_2), Op::Math(MathOp::Tan)];
    let res = vm.eval(&code, &[], 0.0);
    // tan(pi/2) may or may not be exactly Inf due to float precision;
    // if finite, it's a very large number which is acceptable
    if let Ok(val) = res {
        assert!(
            val.is_finite(),
            "If tan(pi/2) returns Ok, it must be finite"
        );
    }
}

#[test]
fn test_vm_finite_arithmetic_still_works() {
    let mut vm = VirtualMachine::new();

    // Normal operations should still succeed
    let code = vec![Op::Push(100.0), Op::Push(200.0), Op::Add];
    assert_eq!(vm.eval(&code, &[], 0.0).unwrap(), 300.0);

    let code = vec![Op::Push(10.0), Op::Push(3.0), Op::Div];
    let res = vm.eval(&code, &[], 0.0).unwrap();
    assert!((res - 10.0 / 3.0).abs() < 1e-10);
}
