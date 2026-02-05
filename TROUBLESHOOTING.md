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
8. [Performance Issues](#performance-issues)
9. [Common Patterns](#common-patterns)

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
// ❌ Wrong
sys.add_rule("A B")?;

// ✅ Correct
sys.add_rule("A -> B")?;
```

#### Missing Colon Before Condition
```rust
// ❌ Wrong
sys.add_rule("A(x) x > 5 -> B(x)")?;

// ✅ Correct
sys.add_rule("A(x) : x > 5 -> B(x)")?;
```

#### Invalid Context Syntax
```rust
// ❌ Wrong
sys.add_rule("A < B < C -> D")?;  // Double left context

// ✅ Correct
sys.add_rule("A < B > C -> D")?;
```

### 2. NaN or Infinity in Input

**Error:**
```
Parser error: verify failed
```

**Cause:**
```rust
// ❌ Wrong
sys.set_axiom("A(NaN)")?;
sys.set_axiom("A(inf)")?;
sys.set_axiom("A(infinity)")?;

// ✅ Correct
sys.set_axiom("A(0.0)")?;
sys.set_axiom("A(1e308)")?;  // Large but finite
```

**Why:** Symbios rejects non-finite floats to prevent undefined behavior.

### 3. Identifier Too Long

**Error:**
```
Parser error: identifier exceeds maximum length
```

**Cause:**
```rust
// ❌ Wrong - identifier > 64 characters
sys.add_rule("very_long_identifier_that_exceeds_the_maximum_allowed_length_of_64_chars -> B")?;

// ✅ Correct
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
// ❌ Wrong - > 32 parameters
sys.set_axiom("A(1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33)")?;

// ✅ Correct
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
// ❌ Wrong - deeply nested expression
sys.add_rule("A(x) -> B(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(f(x))))))))))))))))")?;

// ✅ Correct
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
// ❌ Wrong - context symbols cannot have parameters
sys.add_rule("A(x) < B(y) > C(z) -> D")?;

// ✅ Correct
sys.add_rule("A < B(y) > C -> D")?;  // Only B has parameters
```

**Why:** Left and right context symbols are matched by symbol only, not parameters.

### 2. Undefined Variable in Expression

**Error:**
```
Compilation error: Undefined variable 'foo'
```

**Cause:**
```rust
sys.add_rule("A(x) -> B(y)")?;  // 'y' not defined

// ✅ Correct
sys.add_rule("A(x) -> B(x)")?;  // Use 'x' from predecessor
```

**Available Variables:**
- Predecessor parameters: `A(x, y, z)` defines `x`, `y`, `z`
- Built-in: `age` (module age in time units)
- Constants: Defined via `#define`

### 3. Undefined Constant

**Error:**
```
Compilation error: Undefined constant 'FOO'
```

**Cause:**
```rust
sys.add_rule("A -> B(FOO)")?;  // FOO not defined

// ✅ Correct
sys.add_rule("#define FOO 42")?;
sys.add_rule("A -> B(FOO)")?;
```

### 4. Too Many Successors

**Error:**
```
Compilation error: too many successors (limit 128)
```

**Cause:**
```rust
// ❌ Wrong - > 128 modules in successor
sys.add_rule("A -> B B B B ... (129 times) ... B")?;

// ✅ Correct
sys.add_rule("A -> B C D")?;
```

**Limit:** 128 modules per rule successor.

---

## Runtime Errors

### 1. VM Stack Overflow

**Error:**
```
VM error: Stack overflow
```

**Cause:**
```rust
// ❌ Wrong - expression too complex
sys.add_rule("A(x) -> B(x + x + x + x + ... (100+ terms))")?;

// ✅ Correct - simplify expression
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
sys.add_rule("A(x) : x / 0 -> B")?;

// ✅ Fix
sys.add_rule("A(x) : x != 0 -> B(1/x)")?;
```

#### Sqrt of Negative
```rust
sys.add_rule("A(x) -> B(sqrt(x))")?;

// ✅ Fix
sys.add_rule("A(x) : x >= 0 -> B(sqrt(x))")?;
```

#### Overflow
```rust
sys.add_rule("A(x) -> B(x ^ 1000)")?;

// ✅ Fix
sys.add_rule("A(x) : x < 10 -> B(x ^ 2)")?;
```

### 3. Parameter Out of Bounds

**Error:**
```
VM error: Parameter index out of bounds
```

**Cause:**
```rust
// Internal error - usually indicates a bug in Symbios
// Report this if encountered
```

**Solution:** Ensure rule parameters match axiom/predecessor arity.

---

## State Errors

### 1. Parameter Overflow

**Error:**
```
Parameter count N exceeds limit M
```

**Cause:**
```rust
// ❌ Wrong - trying to create module with > 65535 parameters
// This is extremely unlikely in normal use

// ✅ Limit
// 65,535 parameters per module (u16::MAX)
```

### 2. Unmatched Bracket

**Error:**
```
Unmatched bracket at index N
```

**Cause:**
```rust
// ❌ Wrong
sys.set_axiom("A [ B C")?;  // Missing ]
sys.set_axiom("A ] B C")?;  // Extra ]
sys.set_axiom("A ] B [ C")?;  // Reversed order

// ✅ Correct
sys.set_axiom("A [ B C ]")?;
```

**Note:** Brackets are processed **after derivation**. If your rules produce unbalanced brackets, you'll get this error when `calculate_topology` is called.

### 3. Ambiguous Topology

**Error:**
```
Ambiguous topology symbols
```

**Cause:**
```rust
// ❌ Wrong - same symbol for open and close
let open = sys.interner.get_or_intern("X")?;
let close = sys.interner.get_or_intern("X")?;
sys.state.calculate_topology(open, close)?;

// ✅ Correct
let open = sys.interner.get_or_intern("[")?;
let close = sys.interner.get_or_intern("]")?;
sys.state.calculate_topology(open, close)?;
```

### 4. Max Nesting Depth Exceeded

**Error:**
```
Max nesting depth exceeded
```

**Cause:**
```rust
// ❌ Wrong - nesting > 4096 levels
sys.set_axiom("[ [ [ [ ... (4097 times) ... ] ] ] ]")?;

// ✅ Correct - limit nesting
sys.set_axiom("[ [ [ A ] ] ]")?;
```

**Limit:** 4,096 levels of bracket nesting.

### 5. State Capacity Overflow

**Error:**
```
State capacity overflow
```

**Cause:**
```rust
// ❌ Problem - exponential growth
sys.add_rule("A -> A A")?;  // Doubles every step
sys.set_axiom("A")?;
sys.derive(30)?;  // 2^30 = 1 billion modules!

// ✅ Solution 1 - Increase capacity
sys.max_capacity = 10_000_000;

// ✅ Solution 2 - Add growth limits
sys.add_rule("A(n) : n < 20 -> A(n+1) A(n+1)")?;
sys.set_axiom("A(0)")?;
sys.derive(30)?;  // Stops growing at n=20
```

**Default Limit:** 1,000,000 modules.

---

## Interner Errors

### 1. Heap Overflow

**Error:**
```
Interner error: Heap overflow (exceeded 10485760 bytes)
```

**Cause:** Too many unique symbol strings loaded.

**Example:**
```rust
// ❌ Problem
for i in 0..1_000_000 {
    sys.interner.get_or_intern(&format!("symbol_{}", i))?;
}

// ✅ Solution - reuse symbols
sys.interner.get_or_intern("A")?;
sys.interner.get_or_intern("B")?;
sys.interner.get_or_intern("C")?;
```

**Limit:** 10 MB of string data.

**Adjustment:**
```rust
// Increase heap limit (if needed)
sys.interner.max_heap = 100_000_000;  // 100 MB
```

### 2. Interner Full

**Error:**
```
Interner error: Interner full (max 65535 symbols)
```

**Cause:** More than 65,535 unique symbols.

**Solution:** Reduce symbol diversity or use parameters instead.

```rust
// ❌ Wrong
// A1, A2, A3, ..., A100000

// ✅ Correct
// A(1), A(2), A(3), ..., A(100000)
```

---

## VM Errors

### 1. Stack Underflow

**Error:**
```
Stack underflow
```

**Cause:** Internal error - indicates a compiler bug.

**Solution:** Report issue with rule that caused it.

### 2. Empty Stack

**Error:**
```
Stack empty at result time
```

**Cause:** Expression produced no result.

**Solution:** Report issue with rule that caused it.

---

## Performance Issues

### Symptom: Derivation is Very Slow

**Diagnosis:**
```rust
use std::time::Instant;

let start = Instant::now();
sys.derive(10)?;
println!("Derivation took: {:?}", start.elapsed());
```

**Common Causes:**

1. **Too Many Modules**
```rust
println!("Module count: {}", sys.state.len());
// If > 100,000, consider reducing derivation depth
```

2. **Too Many Rules**
```rust
println!("Rule count: {}", sys.rules.values().map(|v| v.len()).sum::<usize>());
// Each module is tested against every rule
```

3. **Complex Expressions**
```rust
// ❌ Slow
sys.add_rule("A(x) -> B(sin(cos(tan(sqrt(abs(x))))))")?;

// ✅ Fast
sys.add_rule("A(x) -> B(x * 2)")?;
```

**See:** [PERFORMANCE.md](PERFORMANCE.md) for optimization tips.

### Symptom: Memory Usage Too High

**Diagnosis:**
```rust
let module_count = sys.state.len();
let avg_params = 3.0;  // Estimate
let bytes_per_module = 16 + (avg_params * 8.0);
let estimated_mb = (module_count as f64 * bytes_per_module) / 1_000_000.0;
println!("Estimated state size: {:.2} MB", estimated_mb);
```

**Solutions:**
- Reduce derivation depth
- Add pruning rules
- Set `max_capacity` to catch runaway growth early

---

## Common Patterns

### Pattern 1: Debugging Rule Matching

**Problem:** Rule doesn't seem to fire.

**Debug Steps:**
```rust
// 1. Check axiom
println!("Axiom: {}", sys.export_string()?);

// 2. Derive one step
sys.derive(1)?;
println!("After 1 step: {}", sys.export_string()?);

// 3. Check if symbol exists
let sym_id = sys.interner.get_or_intern("A")?;
let rules = sys.rules.get(&sym_id);
println!("Rules for 'A': {:?}", rules);

// 4. Check condition
// Add debug output to condition
sys.add_rule("A(x) : x > 5 -> B(x)")?;
// If x is never > 5, rule won't fire
```

### Pattern 2: Validating Output

**Problem:** Output seems wrong.

**Steps:**
```rust
// 1. Export to string
let output = sys.export_string()?;
println!("Output: {}", output);

// 2. Check module count
println!("Module count: {}", sys.state.len());

// 3. Manual inspection
for i in 0..sys.state.len().min(10) {
    let view = sys.state.get_view(i).unwrap();
    let sym = sys.interner.resolve(view.sym).unwrap();
    println!("Module {}: {} {:?}", i, sym, view.params);
}
```

### Pattern 3: Handling Stochastic Rules

**Problem:** Non-deterministic output.

**Solution:**
```rust
// Set seed for reproducibility
sys.set_seed(12345);

sys.add_rule("A : rand() < 0.5 -> B")?;
sys.add_rule("A : rand() >= 0.5 -> C")?;
sys.set_axiom("A A A A")?;
sys.derive(1)?;

// Same seed = same output
let output1 = sys.export_string()?;

sys.set_seed(12345);
sys.set_axiom("A A A A")?;
sys.derive(1)?;
let output2 = sys.export_string()?;

assert_eq!(output1, output2);  // Deterministic
```

---

## Getting Help

If this guide doesn't resolve your issue:

1. **Check Examples:** Review [examples/](examples/) for working code
2. **Read Tests:** See [tests/](tests/) for edge case handling
3. **File an Issue:** Include:
   - Minimal reproducible example
   - Full error message
   - Symbios version
   - Rust version (`rustc --version`)

---

## Error Reference Table

| Error Type | Prefix | Source File |
|-----------|--------|-------------|
| ParseError | `Parser error:` | [src/parser/mod.rs](src/parser/mod.rs) |
| CompileError | `Compilation error:` | [src/system.rs](src/system.rs) |
| InternerError | `Interner error:` | [src/core/interner.rs](src/core/interner.rs) |
| VMError | `VM error:` | [src/vm/mod.rs](src/vm/mod.rs) |
| SymbiosError | `State error:` | [src/core/mod.rs](src/core/mod.rs) |
| SystemError | (various) | [src/system.rs](src/system.rs) |

---

## Prevention Checklist

Before deploying your L-System:

- [ ] All rules parse without errors
- [ ] Axiom is valid
- [ ] Conditions use defined variables only
- [ ] Expressions are finite (no NaN/Inf)
- [ ] Brackets are balanced
- [ ] Growth is bounded (check `state.len()` periodically)
- [ ] Stochastic rules have `set_seed` for testing
- [ ] Max capacity is set appropriately
- [ ] Tests pass: `cargo test`
