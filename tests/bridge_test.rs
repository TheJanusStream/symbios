use symbios::System;
use symbios::vm::Op;

#[test]
fn test_bridge_compilation() {
    let mut system = System::new();

    // 1. Define a Parametric Rule
    // A(x, y) : x > 5 -> B(x * 2) C(y - 1)
    let rule_src = "A(x, y) : x > 5 -> B(x * 2) C(y - 1)";

    system.add_rule(rule_src).expect("Failed to add rule");

    // 2. Verification

    // Symbols should be interned
    assert_eq!(system.interner.resolve(0), Some("A"));
    assert_eq!(system.interner.resolve(1), Some("B"));
    assert_eq!(system.interner.resolve(2), Some("C"));

    // Check Rule Structure
    let rule = &system.rules[0];
    assert_eq!(rule.predecessor, 0); // Symbol A is 0
    assert_eq!(rule.param_count, 2); // x, y

    // Check Condition Bytecode: x > 5
    // Expect: [LoadParam(0), Push(5.0), Gt]
    let cond = rule.condition.as_ref().unwrap();
    assert_eq!(cond.len(), 3);
    assert_eq!(cond[0], Op::LoadParam(0)); // x
    assert_eq!(cond[1], Op::Push(5.0));
    assert_eq!(cond[2], Op::Gt);

    // Check Successors
    assert_eq!(rule.successors.len(), 2);

    // Successor 1: B(x * 2)
    let b = &rule.successors[0];
    assert_eq!(b.symbol, 1); // B
    // Param: x * 2 -> [Load(0), Push(2.0), Mul]
    assert_eq!(b.params[0][0], Op::LoadParam(0));
    assert_eq!(b.params[0][1], Op::Push(2.0));
    assert_eq!(b.params[0][2], Op::Mul);
}

#[test]
fn test_axiom_loading() {
    let mut system = System::new();
    system
        .set_axiom("A(10) B(20, 30)")
        .expect("Failed to set axiom");

    // Check State
    // View 0 -> A(10)
    let v1 = system.state.get_view(0).unwrap();
    assert_eq!(system.interner.resolve(v1.sym), Some("A"));
    assert_eq!(v1.params, &[10.0]);

    // View 1 -> B(20, 30)
    let v2 = system.state.get_view(1).unwrap();
    assert_eq!(system.interner.resolve(v2.sym), Some("B"));
    assert_eq!(v2.params, &[20.0, 30.0]);
}
