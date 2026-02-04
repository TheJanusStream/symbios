use symbios::System;
use symbios::system::matching::{self, MatchScratch};
use symbios::vm::VirtualMachine;

/// Helper to set up a system state for testing
fn setup_state(sys: &mut System, axiom: &str) {
    sys.set_axiom(axiom).expect("Failed to set axiom");

    let open = sys.interner.get_or_intern("[").expect("Intern failed");
    let close = sys.interner.get_or_intern("]").expect("Intern failed");

    sys.state
        .calculate_topology(open, close)
        .expect("Topology calc failed");
}

#[test]
fn test_stateless_context_1l_1r() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    let mut scratch = MatchScratch::new();

    sys.add_rule("A < B > C -> X").unwrap();

    let b_id = sys.interner.resolve_id("B").expect("B not interned");
    let rule = sys.rules[&b_id][0].clone();

    setup_state(&mut sys, "A B C");

    let is_match = matching::matches(
        &sys.state,
        1, // Index of 'B'
        &rule,
        &sys.ignored_symbols,
        &mut vm,
        &mut scratch,
    )
    .expect("Match execution failed");

    assert!(is_match, "B should match context A < B > C");
}

#[test]
fn test_parametric_context_aggregation() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    let mut scratch = MatchScratch::new();

    sys.add_rule("L(a) < P(b) > R(c) : a + b + c == 30 -> S")
        .unwrap();

    let p_id = sys.interner.resolve_id("P").expect("P not interned");
    let rule = sys.rules.get(&p_id).unwrap()[0].clone();

    setup_state(&mut sys, "L(10) P(5) R(15)");
    let is_match = matching::matches(
        &sys.state,
        1,
        &rule,
        &sys.ignored_symbols,
        &mut vm,
        &mut scratch,
    )
    .expect("Match execution failed");
    assert!(is_match);

    setup_state(&mut sys, "L(10) P(5) R(20)");
    let is_match_neg = matching::matches(
        &sys.state,
        1,
        &rule,
        &sys.ignored_symbols,
        &mut vm,
        &mut scratch,
    )
    .expect("Match execution failed");
    assert!(!is_match_neg);
}

#[test]
fn test_branch_skipping_abop_compliance() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    let mut scratch = MatchScratch::new();

    sys.add_rule("A > B -> X").unwrap();
    let a_id = sys.interner.resolve_id("A").expect("A not interned");
    let rule = sys.rules.get(&a_id).unwrap()[0].clone();

    setup_state(&mut sys, "A [ I ] B");
    let is_match = matching::matches(
        &sys.state,
        0,
        &rule,
        &sys.ignored_symbols,
        &mut vm,
        &mut scratch,
    )
    .unwrap();
    assert!(is_match);
}

#[test]
fn test_nested_branch_skipping() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    let mut scratch = MatchScratch::new();

    sys.add_rule("A > B -> X").unwrap();
    let a_id = sys.interner.resolve_id("A").unwrap();
    let rule = sys.rules.get(&a_id).unwrap()[0].clone();

    setup_state(&mut sys, "A [ X [ Y ] Z ] B");
    let is_match = matching::matches(
        &sys.state,
        0,
        &rule,
        &sys.ignored_symbols,
        &mut vm,
        &mut scratch,
    )
    .unwrap();
    assert!(is_match);
}

#[test]
fn test_parameter_alignment_hazard() {
    let mut sys = System::new();
    let mut vm = VirtualMachine::new();
    let mut scratch = MatchScratch::new();

    sys.add_rule("A(x) > B(y) : x < y -> X").unwrap();
    let a_id = sys.interner.resolve_id("A").unwrap();
    let rule = sys.rules.get(&a_id).unwrap()[0].clone();

    setup_state(&mut sys, "A(10) B(20)");
    let res = matching::matches(
        &sys.state,
        0,
        &rule,
        &sys.ignored_symbols,
        &mut vm,
        &mut scratch,
    )
    .unwrap();
    assert!(res);

    setup_state(&mut sys, "A(10, 5) B(20)");
    let res_hazard = matching::matches(
        &sys.state,
        0,
        &rule,
        &sys.ignored_symbols,
        &mut vm,
        &mut scratch,
    )
    .unwrap();
    assert!(!res_hazard);
}

#[test]
fn test_child_cannot_see_parent() {
    let mut sys = System::new();

    // Rule: If B is preceded by A, transform into Success (S)
    sys.add_rule("A < B -> S").unwrap();
    sys.set_axiom("A [ B ]").unwrap();

    // Derive 1 step
    sys.derive(1).unwrap();

    let output = format!("{}", sys.state.display(&sys.interner));

    assert_eq!(
        output, "A [ S ]",
        "Logic Defect: Module 'B' inside branch failed to see parent 'A' through '['."
    );
}

/// Documents that topology-based branch skipping takes precedence over `#ignore`.
///
/// When brackets (`[` and `]`) are present in the state, `derive()` automatically
/// calls `calculate_topology()`, which enables branch-aware context matching.
/// The topology skip logic takes precedence over the `ignore` list, meaning
/// `#ignore : [ ]` is silently ineffective for bracket symbols.
///
/// This is intentional behavior for correct L-System context matching as described
/// in ABOP (The Algorithmic Beauty of Plants). The topology logic ensures that:
/// - A `]` causes a jump to its matching `[` (skipping sibling branches)
/// - A `[` causes a jump to its matching `]` (skipping child branches)
///
/// This test documents this behavior so users understand that:
/// 1. `#ignore : [ ]` will NOT make brackets ignorable like other symbols
/// 2. Brackets always participate in topology-aware branch skipping during derive()
#[test]
fn test_topology_precedence_over_ignore_directive() {
    let mut sys = System::new();
    sys.add_directive("#ignore : [ ]").unwrap();
    sys.add_rule("A > B -> S").unwrap();
    sys.set_axiom("A [ X ] B").unwrap();

    // derive() automatically calculates topology when brackets are present
    sys.derive(1).unwrap();
    let output = format!("{}", sys.state.display(&sys.interner));

    // Despite #ignore : [ ], brackets still enable branch-aware matching.
    // A matches B by skipping the branch [X] via topology, not via ignore.
    assert_eq!(
        output, "S [ X ] B",
        "Topology takes precedence: A > B matches because [X] is skipped via topology, \
         not because brackets are ignored. #ignore : [ ] is ineffective for brackets."
    );
}

/// Documents that #ignore works correctly for non-bracket symbols.
///
/// Unlike brackets which are handled by topology, regular symbols in the
/// ignore list are truly skipped during context matching.
#[test]
fn test_ignore_directive_works_for_regular_symbols() {
    let mut sys = System::new();
    sys.add_directive("#ignore : X").unwrap();
    sys.add_rule("A > B -> S").unwrap();
    sys.set_axiom("A X B").unwrap();

    sys.derive(1).unwrap();
    let output = format!("{}", sys.state.display(&sys.interner));

    // X is ignored, so A > B matches
    assert_eq!(
        output, "S X B",
        "#ignore : X should allow A > B to match by skipping X"
    );
}
