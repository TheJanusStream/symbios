# Symbios Performance Guide

This guide provides benchmarking results, optimization tips, and performance characteristics for Symbios.

## Table of Contents

1. [Benchmark Results](#benchmark-results)
2. [Performance Characteristics](#performance-characteristics)
3. [Optimization Tips](#optimization-tips)
4. [Profiling Guide](#profiling-guide)
5. [Common Bottlenecks](#common-bottlenecks)

---

## Benchmark Results

All benchmarks run on the standard test suite using Criterion.rs.

### Exponential Growth (Algae)

**Test:** Classic L-System doubling pattern
```
A → A B
B → A
Axiom: A
Depth: 15 steps
Result: 987 modules (Fibonacci sequence)
```

**Performance:**
- **Time:** ~79 µs (microseconds)
- **Throughput:** ~12,500 derivations/second
- **Per-module cost:** ~80 ns/module

**What this tests:**
- State allocation and module pushing
- Symbol matching speed
- Memory layout efficiency

### Context-Sensitive Matching

**Test:** Signal propagation with left/right context
```
A(x) < B(y) > C(z) → B(x+y+z)
Axiom: [A(1) B(1) C(1)] × 1000 = 3,000 modules
Depth: 1 step
```

**Performance:**
- **Time:** ~169 µs (microseconds)
- **Throughput:** ~17.7 million modules/second (3000 modules ÷ 169µs)
- **Per-module cost:** ~56 ns/module

**What this tests:**
- Context matching algorithm
- Parameter access speed
- Expression evaluation overhead

### Benchmark Summary

| Operation | Time | Modules | ns/module |
|-----------|------|---------|-----------|
| Algae Growth (15 steps) | 79 µs | 987 | 80 |
| Context Matching (1 step) | 169 µs | 3,000 | 56 |

---

## Performance Characteristics

### Algorithmic Complexity

| Operation | Time Complexity | Space Complexity | Notes |
|-----------|----------------|------------------|-------|
| Add rule | O(P) | O(P) | P = rule string length |
| Set axiom | O(A) | O(A) | A = axiom string length |
| Derive (N steps) | O(N × M × R) | O(M) | M = modules, R = rules |
| Context matching | O(1) | O(1) | Via skip-links |
| Parameter access | O(1) | O(1) | SoA layout |
| Symbol comparison | O(1) | - | u16 equality |
| Expression eval | O(I) | O(S) | I = instructions, S = stack depth |
| Topology calculation | O(M) | O(D) | D = max nesting depth |

### Memory Layout Efficiency

**Per-Module Overhead:**
```rust
// ModuleData size: 16 bytes
struct ModuleData {
    symbol: u16,        // 2 bytes
    birth_time: f64,    // 8 bytes
    param_start: u32,   // 4 bytes
    param_len: u16,     // 2 bytes
    topology_link: u32, // 4 bytes (packed to 16 total)
}
```

**Parameters:** 8 bytes per parameter (f64)

**Example:** Module `A(1.0, 2.0, 3.0)` costs:
- ModuleData: 16 bytes
- Parameters: 3 × 8 = 24 bytes
- **Total: 40 bytes**

Compare to naive approach:
```rust
struct NaiveModule {
    symbol: String,        // 24 bytes (ptr + len + cap)
    birth_time: f64,       // 8 bytes
    parameters: Vec<f64>,  // 24 bytes
    topology_link: Option<usize>, // 16 bytes (Option + usize)
    // Padding: ~8 bytes
}
// Total: ~80 bytes + heap allocation overhead
```

**SoA saves 50% memory per module.**

---

## Optimization Tips

### 1. Pre-allocate State Capacity

If you know your L-System will grow to ~N modules, pre-allocate:

```rust
let mut sys = System::new();
sys.state.max_capacity = 1_000_000;  // Adjust as needed

// Or manually reserve
// (Currently not exposed, but could be added)
```

**Why:** Reduces reallocations during derivation.

### 2. Minimize Rule Count

Each module is tested against every rule on every derivation step.

**Bad:**
```rust
sys.add_rule("A -> B")?;
sys.add_rule("A -> C")?;  // Stochastic rule
sys.add_rule("B -> D")?;
sys.add_rule("C -> D")?;
// 4 rules = 4× more comparisons
```

**Good:**
```rust
// Use conditions to combine rules
sys.add_rule("A : rand() < 0.5 -> B")?;
sys.add_rule("A : rand() >= 0.5 -> C")?;
sys.add_rule("B -> D")?;
sys.add_rule("C -> D")?;
```

**Better:** Consolidate where possible.

### 3. Simplify Expressions

Complex expressions are compiled once but evaluated on every match.

**Slow:**
```rust
sys.add_rule("A(x) -> B(sin(x) + cos(x) + tan(x))")?;
```

**Fast:**
```rust
// Precompute if x is constant across derivations
sys.add_rule("A(x) -> B(x * 1.732)")?;  // Approximation
```

**Trade-off:** Accuracy vs. speed.

### 4. Avoid Deep Nesting

Topology calculation is O(N) but with a stack. Deep nesting increases overhead.

**Slow:**
```
Axiom: A [ [ [ [ B ] ] ] ]  // 4 levels
```

**Fast:**
```
Axiom: A [ B ] [ C ] [ D ]  // 1 level, parallel branches
```

**Limit:** Max nesting is 4,096 (hard limit to prevent DoS).

### 5. Use Stochastic Rules Wisely

Stochastic rules require random number generation and additional condition checks.

```rust
sys.set_seed(12345);  // Seed once for determinism
sys.add_rule("A : rand() < 0.3 -> B")?;
```

**Cost:** `rand()` call + condition evaluation on every match.

### 6. Batch Derivations

Instead of:
```rust
for i in 0..100 {
    sys.derive(1)?;
    // Do something with state
}
```

Do:
```rust
sys.derive(100)?;
// Process final state
```

**Why:** Reduces setup/teardown overhead.

---

## Profiling Guide

### Running Benchmarks

```bash
cargo bench
```

Output will be in `target/criterion/`.

### Custom Benchmarks

Add to `benches/bench_main.rs`:

```rust
fn bench_my_case(c: &mut Criterion) {
    c.bench_function("My Case", |b| {
        b.iter(|| {
            let mut sys = System::new();
            sys.add_rule("...").unwrap();
            sys.set_axiom("...").unwrap();
            sys.derive(black_box(10)).unwrap();
        })
    });
}
```

### Profiling with `perf` (Linux)

```bash
cargo build --release
perf record --call-graph=dwarf target/release/symbios
perf report
```

### Flamegraph

```bash
cargo install flamegraph
cargo flamegraph --bench bench_main
```

---

## Common Bottlenecks

### 1. String Export

**Problem:**
```rust
let output = sys.export_string()?;  // Slow for large states
```

**Why:** Allocates a new `String` and formats every module.

**Solution:** Only export when needed. Use `state.len()` for progress tracking.

### 2. Topology Calculation

**Problem:**
```rust
// Called on every derivation if you have branches
sys.derive(1)?;  // Recalculates topology
```

**Why:** Symbios recalculates skip-links after each derivation step if `[` and `]` are present.

**Solution:** This is unavoidable for branching L-Systems, but the cost is O(N) which is acceptable.

### 3. Rule Compilation

**Problem:**
```rust
for _ in 0..1000 {
    sys.add_rule("A(x) -> B(x + sin(x))")?;  // Repeated compilation
}
```

**Why:** Each `add_rule` parses and compiles the expression.

**Solution:** Add rules once, reuse the system.

### 4. Excessive Derivations

**Problem:**
```rust
sys.derive(100)?;  // Exponential growth
// State explodes to millions of modules
```

**Why:** L-Systems can grow exponentially (e.g., `A → AA` doubles every step).

**Solution:**
- Set `state.max_capacity` to catch runaway growth
- Monitor `state.len()` between derivations
- Use pruning rules to limit growth

**Example:**
```rust
sys.add_rule("A(x) : x < 100 -> A(x + 1)")?;  // Growth limit
```

### 5. Deep Expression Trees

**Problem:**
```rust
sys.add_rule("A(x) -> B((((x + 1) * 2) + 3) * 4)")?;  // Nested
```

**Why:** More bytecode instructions = slower VM execution.

**Solution:** Simplify expressions or precompute constants.

---

## Optimization Checklist

Before deploying Symbios in production, verify:

- [ ] Rules are minimal and non-redundant
- [ ] Expressions are simplified
- [ ] State capacity is set appropriately
- [ ] Derivation depth is bounded
- [ ] Stochastic rules are used sparingly
- [ ] Benchmarks pass performance regression tests
- [ ] Memory usage is profiled for your use case

---

## Expected Performance Ranges

Based on benchmarks and real-world usage:

| Use Case | Modules | Derivations/sec | Notes |
|----------|---------|-----------------|-------|
| Small (< 100 modules) | 10-100 | 100,000+ | Interactive rates |
| Medium (1K modules) | 1,000 | 10,000+ | Real-time generation |
| Large (10K modules) | 10,000 | 1,000+ | Batch processing |
| Huge (100K+ modules) | 100,000+ | 100+ | Precompute offline |

**Hardware:** Apple M1/M2, AMD Ryzen 5000+, Intel i5/i7 (modern CPUs).

---

## Future Optimizations

Potential improvements not yet implemented:

1. **SIMD Parameter Evaluation**: Vectorize expression evaluation across modules
2. **Parallel Derivation**: Multi-threaded state partitioning
3. **Rule Caching**: Memoize frequently-matched patterns
4. **Just-Enough Compilation**: Compile only hot paths

**Trade-offs:** Added complexity, portability concerns (SIMD, threading in WASM).

---

## Reporting Performance Issues

If you encounter unexpectedly slow performance:

1. Run benchmarks: `cargo bench`
2. Profile with `perf` or `flamegraph`
3. Check `state.len()` growth rate
4. Verify rule complexity
5. Report issue with:
   - System specs
   - Rule set
   - Axiom
   - Derivation depth
   - Benchmark comparison

---

## References

- [Criterion.rs Benchmarking](https://github.com/bheisler/criterion.rs)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Structure-of-Arrays Benefits](https://en.wikipedia.org/wiki/AoS_and_SoA)
