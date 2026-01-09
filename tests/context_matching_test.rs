use symbios::System;
use symbios::system::matching;
use symbios::vm::VirtualMachine;

/// Helper to bootstrap a system with symbols interned and topology calculated
fn setup_system(axiom: &str) -> (System, u16, u16, u16, u16, u16) {
    let mut sys = System::new();
    sys.set_axiom(axiom).expect("Failed to set axiom");

    // Ensure standard symbols are interned so we have their IDs
    let id_a = sys.interner.intern("A").unwrap();
    let id_b = sys.interner.intern("B").unwrap();
    let id_c = sys.interner.intern("C").unwrap();
    let id_open = sys.interner.intern("[").unwrap();
    let id_close = sys.interner.intern("]").unwrap();

    // Critical: Calculate topology so skip links are populated
    sys.state
        .calculate_topology(id_open, id_close)
        .expect("Topology calc failed");

    (sys, id_a, id_b, id_c, id_open, id_close)
}

#[test]
fn test_linear_left_context() {
    // Case: A B
    // Rule: A < B
    let (sys, id_a, id_b, _, _, _) = setup_system("A(1) B(1)");
    let mut vm = VirtualMachine::new();

    // Check B (index 1)
    let is_match = matching::matches(
        &sys.state,
        1,       // Index of B
        id_b,    // Pred: B
        &[id_a], // Left: A
        &[],     // Right: None
        None,    // Condition
        &[],     // Ignore
        &mut vm,
    )
    .unwrap();

    assert!(is_match, "B should match left context A linearly");
}

#[test]
fn test_branch_skip_left_context() {
    // Case: A [ C ] B
    // Rule: A < B
    // ABOP p.32: The bracketed branch [ C ] is a lateral growth.
    // B is structurally attached to A.
    let (sys, id_a, id_b, _, _, _) = setup_system("A(1) [ C(1) ] B(1)");
    let mut vm = VirtualMachine::new();

    // Indices: A=0, [=1, C=2, ]=3, B=4
    let is_match = matching::matches(
        &sys.state,
        4,       // Index of B
        id_b,    // Pred
        &[id_a], // Left: A
        &[],     // Right
        None,
        &[],
        &mut vm,
    )
    .unwrap();

    assert!(is_match, "B should match A, skipping the branch [ C ]");
}

#[test]
fn test_branch_skip_right_context() {
    // Case: B [ A ] C
    // Rule: B > C
    // C is the next segment on the main axis after B.
    let (sys, _, id_b, id_c, _, _) = setup_system("B(1) [ A(1) ] C(1)");
    let mut vm = VirtualMachine::new();

    // Indices: B=0, [=1, A=2, ]=3, C=4
    let is_match = matching::matches(
        &sys.state,
        0,       // Index of B
        id_b,    // Pred
        &[],     // Left
        &[id_c], // Right: C
        None,
        &[],
        &mut vm,
    )
    .unwrap();

    assert!(is_match, "B should match C, skipping the branch [ A ]");
}

#[test]
fn test_nested_branch_skip() {
    // Case: A [ C [ C ] ] B
    // Rule: A < B
    // Should skip the entire nested structure.
    let (sys, id_a, id_b, _, _, _) = setup_system("A(1) [ C(1) [ C(1) ] ] B(1)");
    let mut vm = VirtualMachine::new();

    // A=0, [=1, C=2, [=3, C=4, ]=5, ]=6, B=7
    let is_match = matching::matches(
        &sys.state,
        7, // Index of B
        id_b,
        &[id_a], // Left: A
        &[],
        None,
        &[],
        &mut vm,
    )
    .unwrap();

    assert!(is_match, "B should skip nested branches to find A");
}

#[test]
fn test_ignore_list() {
    // Case: A + + B
    // Rule: A < B (#ignore +)
    // Common in graphical L-systems where rotation symbols shouldn't break context.
    let (mut sys, id_a, id_b, _, _, _) = setup_system("A(1) + + B(1)");
    let id_plus = sys.interner.intern("+").unwrap();
    let mut vm = VirtualMachine::new();

    // Indices: A=0, +=1, +=2, B=3
    let is_match = matching::matches(
        &sys.state,
        3, // Index of B
        id_b,
        &[id_a], // Left: A
        &[],
        None,
        &[id_plus], // Ignore: +
        &mut vm,
    )
    .unwrap();

    assert!(is_match, "B should match A ignoring + symbols");
}

#[test]
fn test_mismatch_fails() {
    // Case: A B
    // Rule: C < B
    let (sys, _, id_b, id_c, _, _) = setup_system("A(1) B(1)");
    let mut vm = VirtualMachine::new();

    let is_match = matching::matches(
        &sys.state,
        1,
        id_b,
        &[id_c], // Looking for C, but A is there
        &[],
        None,
        &[],
        &mut vm,
    )
    .unwrap();

    assert!(!is_match, "Should fail when context doesn't match");
}
