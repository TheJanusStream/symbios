use symbios::System;
use symbios::system::matching;
use symbios::vm::{Op, VirtualMachine};

#[test]
fn test_vm_param_bounds_check() {
    let mut vm = VirtualMachine::new();
    let code = vec![Op::LoadParam(5)];
    let params = vec![1.0];
    let res = vm.eval(&code, &params, 0.0);
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Parameter index out of bounds"));
}

#[test]
fn test_temporal_growth_logic() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    sys.add_rule("A : age > 5.0 -> B")
        .expect("Compiler should accept 'age' keyword");
    let rule = sys.rules[0].clone();
    sys.set_axiom("A").unwrap();
    sys.state.current_time = 2.0;
    let match_early = matching::matches(&sys.state, 0, &rule, &[], &mut vm).expect("Match failed");
    assert!(!match_early, "Should not match when age is 2.0");
    sys.state.current_time = 6.0;
    let match_late = matching::matches(&sys.state, 0, &rule, &[], &mut vm).expect("Match failed");
    assert!(match_late, "Should match when age is 6.0");
}

#[test]
fn test_neighbor_arity_mismatch() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    sys.add_rule("L(x) < P : x > 5 -> P").unwrap();
    let rule = sys.rules[0].clone();
    sys.state.clear();
    let l = sys.interner.get_or_intern("L").unwrap();
    let p = sys.interner.get_or_intern("P").unwrap();
    sys.state.push(l, 0.0, &[10.0, 99.0]).unwrap();
    sys.state.push(p, 0.0, &[]).unwrap();
    sys.state.calculate_topology(100, 101).unwrap();
    let is_match =
        matching::matches(&sys.state, 1, &rule, &[], &mut vm).expect("Execution success");
    assert!(
        !is_match,
        "Should reject context match due to neighbor arity mismatch"
    );
}
