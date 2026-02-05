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

/// Verifies that `#ignore [ ]` disables topology-based branch skipping.
///
/// When brackets are in the ignore list, they are skipped as plain symbols
/// (not followed via topology links), producing linear context matching.
/// With `A [ X ] B` and `#ignore [ ]`, the effective scan from A sees:
/// `[` (skip), `X` (not B, not ignored → mismatch). So `A > B` does NOT match.
#[test]
fn test_ignore_directive_disables_topology_for_brackets() {
    let mut sys = System::new();
    sys.add_directive("#ignore : [ ]").unwrap();
    sys.add_rule("A > B -> S").unwrap();
    sys.set_axiom("A [ X ] B").unwrap();

    sys.derive(1).unwrap();
    let output = format!("{}", sys.state.display(&sys.interner));

    // With #ignore [ ], brackets are skipped as plain symbols.
    // X blocks the match between A and B, so A is NOT replaced.
    assert_eq!(
        output, "A [ X ] B",
        "#ignore [ ] should disable topology: A > B must not match because X is between them"
    );
}

/// Verifies that `#ignore [ ] X` allows matching through branches linearly.
///
/// When both brackets and intervening symbols are ignored, the linear scan
/// skips all of them, allowing context to match across the branch.
#[test]
fn test_ignore_brackets_and_contents_enables_linear_match() {
    let mut sys = System::new();
    sys.add_directive("#ignore : [ ] X").unwrap();
    sys.add_rule("A > B -> S").unwrap();
    sys.set_axiom("A [ X ] B").unwrap();

    sys.derive(1).unwrap();
    let output = format!("{}", sys.state.display(&sys.interner));

    assert_eq!(
        output, "S [ X ] B",
        "#ignore [ ] X should allow A > B to match by skipping [, X, and ] linearly"
    );
}

/// Verifies that `#ignore [ ]` disables topology for left context matching too.
///
/// With `A [ X ] B` and `#ignore [ ]`, left context scan from B sees:
/// `]` (skip), `X` (not A, not ignored → mismatch). So `A < B` does NOT match.
#[test]
fn test_ignore_brackets_disables_topology_left_context() {
    let mut sys = System::new();
    sys.add_directive("#ignore : [ ]").unwrap();
    sys.add_rule("A < B -> S").unwrap();
    sys.set_axiom("A [ X ] B").unwrap();

    sys.derive(1).unwrap();
    let output = format!("{}", sys.state.display(&sys.interner));

    assert_eq!(
        output, "A [ X ] B",
        "#ignore [ ] should disable topology for left context: A < B must not match because X is between them"
    );
}

/// Verifies that without #ignore, topology correctly skips branches for left context.
#[test]
fn test_topology_skips_branches_left_context_without_ignore() {
    let mut sys = System::new();
    sys.add_rule("A < B -> S").unwrap();
    sys.set_axiom("A [ X ] B").unwrap();

    sys.derive(1).unwrap();
    let output = format!("{}", sys.state.display(&sys.interner));

    // Without #ignore, topology links are active: ] jumps to [, then [ is stepped over,
    // revealing A as the left context of B.
    assert_eq!(
        output, "A [ X ] S",
        "Without #ignore, topology should allow A < B to match by skipping [X] branch"
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
