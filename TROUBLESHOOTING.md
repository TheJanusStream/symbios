# Symbios Troubleshooting Guide

This guide helps diagnose and resolve common errors when using Symbios.

## Table of Contents

1. [Quick Diagnosis](#quick-diagnosis)
2. [Parser Errors](#parser-errors)
3. [Compilation Errors](#compilation-errors)
4. [Runtime Errors](#runtime-errors)
5. [State Errors](#state-errors)
6. [Interner Errors](#interner-errors)
7. [VM Errors](#vm-errors)
8. [Source Round-Tripping Errors](#source-round-tripping-errors)
9. [Genetic Operator Errors](#genetic-operator-errors)
10. [Performance Issues](#performance-issues)
11. [Common Patterns](#common-patterns)

---

## Quick Diagnosis

| Error Message | Section | Quick Fix |
|--------------|---------|-----------|
| `Parser error: ...` | [Parser Errors](#parser-errors) | Check rule syntax |
| `Compilation error: ...` | [Compilation Errors](#compilation-errors) | Check expression syntax |
| `Invalid predecessor parameter` | [Compilation Errors](#compilation-errors) | Remove params from context |
| `Interner error: ...` | [Interner Errors](#interner-errors) | Too many symbols |
| `VM error: ...` | [VM Errors](#vm-errors) | Check expression math |
| `Parameter count N exceeds limit M` | [State Errors](#state-errors) | Reduce parameters |
| `Unmatched bracket at index N` | [State Errors](#state-errors) | Balance `[` and `]` |
| `State capacity overflow` | [State Errors](#state-errors) | Increase `max_capacity` |
| `Invalid numeric value` | [State Errors](#state-errors) | Remove NaN/Inf from input |

---

## Parser Errors

### 1. Invalid Rule Syntax

**Error:**
```
Parser error: Nom Error: Error(Error { input: "...", code: Tag })
```

**Common Causes:**

#### Missing Arrow
```rust
// Wrong
sys.add_rule("A B")?;

// Correct
sys.add_rule("A -> B")?;
```

#### Missing Colon Before Condition
```rust
// Wrong
sys.add_rule("A(x) x > 5 -> B(x)")?;

// Correct
sys.add_rule("A(x) : x > 5 -> B(x)")?;
```

#### Invalid Context Syntax
```rust
// Wrong
sys.add_rule("A < B < C -> D")?;  // Double left context

// Correct
sys.add_rule("A < B > C -> D")?;
```

### 2. NaN or Infinity in Input

**Error:**
```
Parser error: verify failed
```
or
```
State error: Invalid numeric value
```

**Cause:**
```rust
// Wrong
sys.set_axiom("A(NaN)")?;
sys.set_axiom("A(inf)")?;
sys.set_axiom("A(infinity)")?;

// Correct
sys.set_axiom("A(0.0)")?;
sys.set_axiom("A(1e308)")?;  // Large but finite
```

**Why:** Symbios rejects non-finite floats to prevent undefined behavior. The `InvalidNumericValue` error is raised during parsing and state operations.

### 3. Identifier Too Long

**Error:**
```
Parser error: identifier exceeds maximum length
```

**Cause:**
```rust
// Wrong - identifier > 64 characters
sys.add_rule("very_long_identifier_that_exceeds_the_maximum_allowed_length_of_64_chars -> B")?;

// Correct
sys.add_rule("short_name -> B")?;
```

**Limit:** 64 characters per identifier.

### 4. Too Many Arguments

**Error:**
```
Parser error: too many arguments
```

**Cause:**
```rust
// Wrong - > 32 parameters
sys.set_axiom("A(1,2,3,...,33)")?;

// Correct - <= 32 parameters
sys.set_axiom("A(1,2,3,4,5)")?;
```

**Limit:** 32 parameters per module.

### 5. Excessive Nesting in Expressions

**Error:**
```
Parser error: maximum recursion depth exceeded
```

**Cause:**
```rust
// Wrong - deeply nested expression
sys.add_rule("A(x) -> B(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(x))))))))))))))))")?;

// Correct
sys.add_rule("A(x) -> B(f(x))")?;
```

**Limit:** 64 levels of recursion in parser.

---

## Compilation Errors

### 1. Invalid Predecessor Parameter

**Error:**
```
Invalid predecessor parameter
```

**Cause:**
```rust
// Wrong - context symbols cannot have parameters
sys.add_rule("A(x) < B(y) > C(z) -> D")?;

// Correct - only predecessor has parameters
sys.add_rule("A < B(y) > C -> D")?;
```

**Why:** Left and right context symbols are matched by symbol only, not by parameters.

### 2. Undefined Variable in Expression

**Error:**
```
Compilation error: Undefined variable 'foo'
```

**Cause:**
```rust
sys.add_rule("A(x) -> B(y)")?;  // 'y' not defined

// Correct
sys.add_rule("A(x) -> B(x)")?;  // Use 'x' from predecessor
```

**Available Variables:**
- Predecessor parameters: `A(x, y, z)` defines `x`, `y`, `z`
- Built-in: `age` (module age, computed as `current_time - birth_time`)
- Constants: Defined via `#define`

### 3. Undefined Constant

**Error:**
```
Compilation error: Undefined constant 'FOO'
```

**Cause:**
```rust
sys.add_rule("A -> B(FOO)")?;  // FOO not defined

// Correct
sys.add_directive("#define FOO 42")?;
sys.add_rule("A -> B(FOO)")?;
```

### 4. Too Many Successors

**Error:**
```
Compilation error: too many successors (limit 128)
```

**Cause:**
```rust
// Wrong - > 128 modules in successor
sys.add_rule("A -> B B B B ... (129 times) ... B")?;

// Correct
sys.add_rule("A -> B C D")?;
```

**Limit:** 128 modules per rule successor (enforced by `MAX_SUCCESSORS`).

---

## Runtime Errors

### 1. VM Stack Overflow

**Error:**
```
VM error: Stack overflow
```

**Cause:**
```rust
// Wrong - expression too complex
sys.add_rule("A(x) -> B(x + x + x + x + ... (100+ terms))")?;

// Correct - simplify expression
sys.add_rule("A(x) -> B(x * 100)")?;
```

**Limit:** VM stack size is 256 items.

### 2. VM Math Error (NaN/Inf)

**Error:**
```
VM error: Mathematical error (NaN/Inf)
```

**Common Causes:**

#### Division by Zero
```rust
sys.add_rule("A(x) -> B(1 / x)")?;
// When x = 0, produces Inf → MathError

// Fix: guard against zero
sys.add_rule("A(x) : x != 0 -> B(1 / x)")?;
```

#### Sqrt of Negative
```rust
sys.add_rule("A(x) -> B(sqrt(x))")?;
// When x < 0, produces NaN → MathError

// Fix: guard against negative
sys.add_rule("A(x) : x >= 0 -> B(sqrt(x))")?;
```

#### Overflow
```rust
sys.add_rule("A(x) -> B(x ^ 1000)")?;
// Large exponents produce Inf → MathError

// Fix: limit exponent
sys.add_rule("A(x) : x < 10 -> B(x ^ 2)")?;
```

### 3. Parameter Out of Bounds

**Error:**
```
VM error: Parameter index out of bounds
```

**Cause:** This is an internal error indicating a mismatch between rule compilation and execution. Ensure rule parameters match axiom/predecessor arity.

**Solution:** If encountered, report with the rule that caused it.

---

## State Errors

### 1. Parameter Overflow

**Error:**
```
Parameter count N exceeds limit M
```

**Cause:** Trying to create a module with more than 65,535 parameters (u16::MAX). Extremely unlikely in normal use.

### 2. Unmatched Bracket

**Error:**
```
Unmatched bracket at index N
```

**Cause:**
```rust
// Wrong
sys.set_axiom("A [ B C")?;  // Missing ]
sys.set_axiom("A ] B C")?;  // Extra ]
sys.set_axiom("A ] B [ C")?;  // Reversed order

// Correct
sys.set_axiom("A [ B C ]")?;
```

**Note:** Brackets are validated when `calculate_topology` is called. If your rules produce unbalanced brackets during derivation, you'll get this error at the next derivation step.

### 3. Ambiguous Topology

**Error:**
```
Ambiguous topology symbols
```

**Cause:**
```rust
// Wrong - same symbol for open and close
let sym = sys.interner.get_or_intern("X")?;
sys.state.calculate_topology(sym, sym)?;

// Correct - distinct symbols
let open = sys.interner.get_or_intern("[")?;
let close = sys.interner.get_or_intern("]")?;
sys.state.calculate_topology(open, close)?;
```

### 4. Max Nesting Depth Exceeded

**Error:**
```
Max nesting depth exceeded
```

**Cause:** Bracket nesting exceeds 4,096 levels. This limit exists to prevent stack exhaustion from adversarial input.

**Solution:** Flatten your branching structure or reduce derivation depth.

### 5. State Capacity Overflow

**Error:**
```
State capacity overflow
```

**Cause:**
```rust
// Problem - exponential growth
sys.add_rule("A -> A A")?;  // Doubles every step
sys.set_axiom("A")?;
sys.derive(30)?;  // 2^30 = 1 billion modules!

// Solution 1 - Increase capacity
sys.max_capacity = 10_000_000;

// Solution 2 - Add growth limits
sys.add_rule("A(n) : n < 20 -> A(n+1) A(n+1)")?;
sys.set_axiom("A(0)")?;
sys.derive(30)?;  // Stops growing at n=20
```

**Default Limit:** 1,000,000 modules.

### 6. Invalid Numeric Value

**Error:**
```
State error: Invalid numeric value
```

**Cause:** A NaN or Infinity value was detected in a state operation (e.g., pushing a module with non-finite parameters).

**Solution:** Guard expressions that can produce non-finite results. See [VM Math Error](#2-vm-math-error-naninf).

---

## Interner Errors

### 1. Heap Overflow

**Error:**
```
Interner error: Heap overflow (exceeded 10485760 bytes)
```

**Cause:** Too many unique symbol strings loaded.

```rust
// Problem
for i in 0..1_000_000 {
    sys.interner.get_or_intern(&format!("symbol_{}", i))?;
}

// Solution - reuse symbols, use parameters for variation
sys.add_rule("A(1) -> B(2)")?;  // Not A1 -> B2
```

**Limit:** 10 MB of string data.

### 2. Interner Full

**Error:**
```
Interner error: Interner full (max 65535 symbols)
```

**Cause:** More than 65,535 unique symbols.

**Solution:** Use parameters instead of unique symbol names.

```rust
// Wrong: A1, A2, A3, ..., A100000
// Correct: A(1), A(2), A(3), ..., A(100000)
```

---

## VM Errors

### 1. Stack Underflow

**Error:**
```
Stack underflow
```

**Cause:** Internal error indicating a compiler bug (generated bytecode pops more values than available).

**Solution:** Report with the rule that caused it.

### 2. Empty Stack

**Error:**
```
Stack empty at result time
```

**Cause:** Expression produced no result (bytecode sequence leaves stack empty).

**Solution:** Report with the rule that caused it.

---

## Source Round-Tripping Errors

### 1. from_source Parse Failure

**Error:**
```
SystemError::ParseError(...)
```

**Cause:** Invalid syntax in source text passed to `System::from_source()`.

**Common issues:**
```rust
// Wrong - missing omega
let sys = System::from_source("A -> B")?;

// Correct - include axiom
let sys = System::from_source("omega: A\nA -> B")?;
```

**Source format:**
```
#define CONST 42          // Constants (optional)
#ignore: F f              // Ignore list (optional)
omega: A(1)               // Axiom (required for derivation)
A(x) : x < 10 -> B(x)    // Rules
0.5: A -> A A             // Stochastic rules
```

### 2. to_source Decompilation Issues

`to_source()` should always succeed on a valid system. If exported source doesn't round-trip correctly:

1. Check that parameter names are preserved (rules added via `add_rule` preserve names from the original source)
2. Verify constants match — `to_source()` emits all stored `#define` constants sorted alphabetically

### 3. SourceGenotype Mutation Failure

**Error:**
```
SystemError from genotype.mutate_with_rng(...)
```

**Cause:** The source text in the genotype is invalid or a mutation produced invalid state.

**Solution:** Validate source text before constructing `SourceGenotype`:
```rust
// Verify source parses cleanly
let genotype = SourceGenotype::new(source);
let sys = genotype.to_system()?;  // Check for errors
```

---

## Genetic Operator Errors

### 1. Crossover Symbol Resolution

**Problem:** Offspring system references symbols that don't exist in the merged interner.

**Why:** Fixed in earlier versions. Crossover now resolves symbols by name through the interner, not by raw u16 ID.

### 2. Structural Mutation Arity Mismatch

**Problem:** After structural mutation, a rule's successor has wrong parameter count.

**Why:** Structural mutation tracks `symbol_arities` and inserts modules with correct arity. If you see arity mismatches, it may be due to symbols that appear only in context (not as predecessors), where arity is unknown.

**Solution:** Ensure all symbols used in successors also appear as predecessors with parameters defined.

### 3. Probability Normalization

**Note:** Rule probabilities are **relative weights**, not absolute probabilities. They don't need to sum to 1.0.

```rust
// These are equivalent:
sys.add_rule("0.3: A -> B")?;
sys.add_rule("0.7: A -> C")?;

// Same as:
sys.add_rule("3: A -> B")?;
sys.add_rule("7: A -> C")?;
```

After mutation, probabilities are clamped to `[0.0, 1.0]` but still function as relative weights.

### 4. Time Overflow in advance_time

**Error:**
```
advance_time error
```

**Cause:** Calling `advance_time` with a value that would cause `current_time` to overflow or become non-finite.

**Solution:** Use reasonable time deltas. The system is hardened against overflow — it returns an error rather than silently wrapping.

---

## Performance Issues

### Symptom: Derivation is Very Slow

**Diagnosis:**
```rust
use std::time::Instant;

let start = Instant::now();
sys.derive(10)?;
println!("Derivation took: {:?}", start.elapsed());
println!("Module count: {}", sys.state.len());
```

**Common Causes:**

1. **Too Many Modules** — If > 100,000, reduce derivation depth
2. **Too Many Rules** — Each module is tested against every rule for its symbol
3. **Complex Expressions** — Simplify VM bytecode

**See:** [PERFORMANCE.md](PERFORMANCE.md) for optimization tips.

### Symptom: Memory Usage Too High

**Diagnosis:**
```rust
let module_count = sys.state.len();
let avg_params = 3.0;  // Estimate
let bytes_per_module = 16.0 + (avg_params * 8.0);
let estimated_mb = (module_count as f64 * bytes_per_module) / 1_000_000.0;
println!("Estimated state size: {:.2} MB", estimated_mb);
```

**Solutions:**
- Reduce derivation depth
- Add pruning rules (conditions that limit growth)
- Set `sys.max_capacity` to catch runaway growth early

### Symptom: Evolutionary Loop is Slow

**Diagnosis:** Check if you're using `SourceGenotype` in a tight loop.

**Solution:** Operate on `System` objects directly and only call `to_source()` when needed for serialization. See [PERFORMANCE.md](PERFORMANCE.md#genetic-operator-performance).

---

## Common Patterns

### Pattern 1: Debugging Rule Matching

**Problem:** Rule doesn't seem to fire.

**Debug Steps:**
```rust
// 1. Check axiom
println!("Axiom: {}", sys.state.display(&sys.interner));

// 2. Derive one step
sys.derive(1)?;
println!("After 1 step: {}", sys.state.display(&sys.interner));

// 3. Export rules to verify they compiled correctly
for (pred, source) in sys.export_rules() {
    println!("{}: {}", pred, source);
}

// 4. Check individual module parameters
for i in 0..sys.state.len().min(10) {
    if let Some(view) = sys.state.get_view(i) {
        let sym = sys.interner.resolve(view.sym).unwrap_or("?");
        println!("Module {}: {}({:?}) age={:.1}", i, sym, view.params, view.age);
    }
}
```

### Pattern 2: Validating Round-Trip Fidelity

**Problem:** `to_source()` output doesn't match original.

**Steps:**
```rust
let original = r#"
#define ANGLE 45
omega: A(1)
A(x) : x < 10 -> A(x + 1) [ +(ANGLE) B ]
"#;

let sys = System::from_source(original)?;
let exported = sys.to_source();
println!("Exported:\n{}", exported);

// Re-parse to verify
let sys2 = System::from_source(&exported)?;
// Compare behavior
```

**Note:** `to_source()` may reformat expressions (e.g., add parentheses for clarity) and re-order constants alphabetically. The **semantics** should be identical even if the text differs.

### Pattern 3: Handling Stochastic Rules

**Problem:** Non-deterministic output.

**Solution:**
```rust
// Set seed for reproducibility
sys.set_seed(12345);

sys.add_rule("0.5: A -> B")?;
sys.add_rule("0.5: A -> C")?;
sys.set_axiom("A A A A")?;
sys.derive(1)?;

let output1 = sys.state.display(&sys.interner).to_string();

// Reset and re-derive with same seed
sys.set_seed(12345);
sys.set_axiom("A A A A")?;
sys.derive(1)?;
let output2 = sys.state.display(&sys.interner).to_string();

assert_eq!(output1, output2);  // Deterministic
```

### Pattern 4: Inspecting Mutated Rules

**Problem:** Need to understand what a mutation did to a rule set.

```rust
let mut sys = System::new();
sys.add_rule("A(x) -> A(x + 1) B(x)")?;
sys.add_directive("#define SCALE 2.0")?;

println!("Before mutation:");
for (pred, src) in sys.export_rules() {
    println!("  {}: {}", pred, src);
}
println!("  Constants: {:?}", sys.constants);

sys.mutate(&MutationConfig::default());

println!("After mutation:");
for (pred, src) in sys.export_rules() {
    println!("  {}: {}", pred, src);
}
println!("  Constants: {:?}", sys.constants);
```

---

## Getting Help

If this guide doesn't resolve your issue:

1. **Check Examples:** Review [examples/](examples/) for working code
2. **Read Tests:** See [tests/](tests/) for edge case handling (22 test files, 220+ tests)
3. **File an Issue:** Include:
   - Minimal reproducible example
   - Full error message
   - Symbios version
   - Rust version (`rustc --version`)

---

## Error Reference Table

| Error Type | Prefix | Source |
|-----------|--------|--------|
| ParseError | `Parser error:` | `src/parser/mod.rs` |
| CompileError | `Compilation error:` | `src/system/mod.rs` |
| InvalidPredecessorParam | `Invalid predecessor parameter` | `src/system/mod.rs` |
| InternerError | `Interner error:` | `src/core/interner.rs` |
| VMError | `VM error:` | `src/vm/mod.rs` |
| SymbiosError | `State error:` | `src/core/mod.rs` |
| SystemError | (various) | `src/system/mod.rs` |
| StateCorruption | `State corruption` | `src/system/derivation.rs` |

---

## Prevention Checklist

Before deploying your L-System:

- [ ] All rules parse without errors
- [ ] Axiom is valid
- [ ] Conditions use defined variables only
- [ ] Expressions are finite (no NaN/Inf paths)
- [ ] Brackets are balanced (including after derivation)
- [ ] Growth is bounded (check `state.len()` periodically)
- [ ] Stochastic rules have `set_seed` for testing
- [ ] Max capacity is set appropriately
- [ ] Genetic operators produce valid offspring (test with `to_source()`)
- [ ] Tests pass: `cargo test`
