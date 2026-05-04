# Symbios Performance Guide

This guide provides benchmarking results, optimization tips, and performance characteristics for Symbios.

## Table of Contents

1. [Benchmark Results](#benchmark-results)
2. [Performance Characteristics](#performance-characteristics)
3. [Optimization Tips](#optimization-tips)
4. [Profiling Guide](#profiling-guide)
5. [Common Bottlenecks](#common-bottlenecks)
6. [Genetic Operator Performance](#genetic-operator-performance)

---

## Benchmark Results

All benchmarks run on the standard test suite using Criterion.rs.

### Exponential Growth (Algae)

**Test:** Classic L-System doubling pattern

```text
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

```text
A(x) < B(y) > C(z) → B(x+y+z)
Axiom: [A(1) B(1) C(1)] × 1000 = 3,000 modules
Depth: 1 step
```

**Performance:**

- **Time:** ~169 µs (microseconds)
- **Throughput:** ~17.7 million modules/second (3000 modules / 169µs)
- **Per-module cost:** ~56 ns/module

**What this tests:**

- Context matching algorithm with topology skip-links
- Parameter access speed (SoA layout)
- VM expression evaluation overhead
- Zero-allocation matching via `MatchScratch`

### Benchmark Summary

| Operation                 | Time   | Modules | ns/module |
| ------------------------- | ------ | ------- | --------- |
| Algae Growth (15 steps)   | 79 µs  | 987     | 80        |
| Context Matching (1 step) | 169 µs | 3,000   | 56        |

---

## Performance Characteristics

### Algorithmic Complexity

| Operation | Time Complexity | Space Complexity | Notes |
| --------- | --------------- | ---------------- | ----- |
| Add rule | O(P) | O(P) | P = rule string length |
| Set axiom | O(A) | O(A) | A = axiom string length |
| Derive (N steps) | O(N × M × R) | O(M) | M = modules, R = rules per symbol |
| Context matching | O(1) | O(1) | Via skip-links |
| Parameter access | O(1) | O(1) | SoA flat indexing |
| Symbol comparison | O(1) | - | u16 equality |
| Expression eval | O(I) | O(S) | I = instructions, S = stack depth |
| Topology calculation | O(M) | O(D) | D = max nesting depth |
| Stochastic selection | O(R) | O(1) | R = matching rules for module |

### Memory Layout Efficiency

**Per-Module Overhead:**

```rust
// ModuleData size: 24 bytes (with alignment padding for f64)
// Enforced by compile-time assertion in src/core/mod.rs.
struct ModuleData {
    symbol: u16,                     // 2 bytes
    birth_time: f64,                 // 8 bytes (requires 8-byte alignment)
    param_start: u32,                // 4 bytes
    param_len: u16,                  // 2 bytes
    topology_links: [u32; 2],        // 8 bytes (one slot per PairKind)
}
// Raw sum: 24 bytes. The pair-kind array reuses the alignment padding the
// previous single-link layout already paid for, so the struct stays at 24.
```

**Topology link slots:** Two `u32` slots per module — see `PairKind::Branch` (`[ ]`) and `PairKind::Polygon` (`{ }`) in `src/core/mod.rs`. A module can hold an independent skip target for each kind without overwrites; only the slot whose pair-kind matches the module's symbol is meaningful in practice.

**Parameters:** 8 bytes per parameter (f64)

**Example:** Module `A(1.0, 2.0, 3.0)` costs:

- ModuleData: 24 bytes
- Parameters: 3 × 8 = 24 bytes
- **Total: 48 bytes**

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

If you know your L-System will grow to ~N modules, set the capacity:

```rust
let mut sys = System::new();
sys.max_capacity = 1_000_000;  // Adjust as needed
```

**Why:** The `max_capacity` both limits growth and signals expected size. Reduces reallocation during derivation.

### 2. Minimize Rule Count

Each module is tested against every rule for its symbol on every derivation step.

```rust
// Consolidate where possible. Fewer rules = fewer comparisons.
sys.add_rule("A(x) : x < 5 -> B(x)")?;
sys.add_rule("A(x) : x >= 5 -> C(x)")?;
```

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

### 4. Avoid Deep Nesting

Topology calculation is O(N) but uses a stack. Deep nesting increases overhead.

**Slow:**

```text
Axiom: A [ [ [ [ B ] ] ] ]  // 4 levels deep
```

**Fast:**

```text
Axiom: A [ B ] [ C ] [ D ]  // 1 level, parallel branches
```

**Limit:** Max nesting is 4,096 (hard limit to prevent DoS).

### 5. Use Constants Instead of Literals

Constants defined with `#define` are inlined at compile time, with no extra cost. They also enable genetic algorithm coupling (constant mutation affects all references).

```rust
sys.add_directive("#define ANGLE 45")?;
sys.add_rule("A -> [ +(ANGLE) B ] [ -(ANGLE) C ]")?;
```

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

**Why:** Reduces per-step setup overhead (topology recalculation, buffer reset).

### 7. Seed RNG for Consistent Benchmarking

```rust
sys.set_seed(42);
```

Eliminates variance from stochastic rule selection in benchmarks.

---

## Profiling Guide

### Running Benchmarks

```bash
cargo bench
```

Output will be in `target/criterion/`.

### Benchmark Groups

The benchmark suite (`benches/bench_main.rs`) covers six groups:

| Group                        | What it Measures                                       |
| ---------------------------- | ------------------------------------------------------ |
| `Derivation`                 | Core derive loop: allocation, matching, state push     |
| `Context Matching`           | Skip-link navigation, parameter loading, VM eval       |
| `Genetic/Mutation`           | Probability and constant perturbation (10-100 rules)   |
| `Genetic/Crossover`          | Uniform crossover with constant blending (10-50 rules) |
| `Genetic/StructuralMutation` | Module insert/delete/swap, bytecode perturbation       |
| `Genetic/CombinedEvolution`  | Full cycle: crossover + mutation + structural mutation |

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

### 1. State Export

**Problem:**

```rust
let output = sys.state.display(&sys.interner).to_string();  // Slow for large states
```

**Why:** Allocates a new `String` and formats every module.

**Solution:** Only export when needed. Use `sys.state.len()` for progress tracking.

### 2. Topology Calculation

**Problem:**

```rust
sys.derive(1)?;  // Recalculates topology each step if brackets are present
```

**Why:** Symbios recalculates skip-links after each derivation step when `[` and `]` are present.

**Solution:** This is unavoidable for branching L-Systems, but the cost is O(N) which is acceptable.

### 3. Rule Compilation

**Problem:**

```rust
for _ in 0..1000 {
    sys.add_rule("A(x) -> B(x + sin(x))")?;  // Repeated compilation
}
```

**Why:** Each `add_rule` parses and compiles the expression.

**Solution:** Add rules once, reuse the system. For evolutionary workloads, use `from_source()` which parses all rules in one pass.

### 4. Excessive Derivations

**Problem:**

```rust
sys.derive(100)?;  // Exponential growth
// State explodes to millions of modules
```

**Why:** L-Systems can grow exponentially (e.g., `A → AA` doubles every step).

**Solution:**

- Set `sys.max_capacity` to catch runaway growth
- Monitor `sys.state.len()` between derivations
- Use pruning rules to limit growth

```rust
sys.add_rule("A(x) : x < 100 -> A(x + 1)")?;  // Growth limit
```

### 5. Source Round-Tripping Overhead

**Problem:** Evolutionary loops using `SourceGenotype` pay parse + compile + decompile cost per generation.

**Why:** Each `mutate_with_rng` call on `SourceGenotype` parses source → builds `System` → applies operator → calls `to_source()`.

**Solution:** For tight loops where source preservation isn't needed, operate directly on `System` objects and only call `to_source()` for serialization/logging.

---

## Genetic Operator Performance

### Relative Cost

| Operator                | Relative Cost | Scales With                               |
| ----------------------- | ------------- | ----------------------------------------- |
| Probability mutation    | Very low      | Number of rules                           |
| Constant mutation       | Very low      | Number of constants                       |
| Gaussian jitter         | Low           | Total bytecode instructions               |
| Operator flip           | Low           | Total bytecode instructions               |
| Structural mutation     | Medium        | Rules × successors per rule               |
| Rule duplication        | Medium        | Number of rules                           |
| Topological mutation    | Low           | Number of successor symbols               |
| Literal promotion       | Medium        | Bytecode instructions × constants         |
| Crossover (uniform)     | Medium        | Total rules across parents                |
| Crossover (homologous)  | Medium        | Shared predecessor symbols                |
| BLX-alpha blending      | Medium        | Shared rule pairs with matching structure |
| Sub-expression grafting | Medium        | Successor sequence length                 |

### Evolutionary Loop Pattern

```rust
// Efficient: operate on System objects directly
let mut population: Vec<System> = /* ... */;
let crossover_config = CrossoverConfig::default();
let mutation_config = MutationConfig::default();

for generation in 0..1000 {
    // Select parents, crossover, mutate
    let mut offspring = population[0].crossover(&population[1], &crossover_config)?;
    offspring.mutate(&mutation_config);

    // Evaluate fitness (derive + measure)
    offspring.set_axiom("A(0)")?;
    offspring.derive(10)?;
    let fitness = offspring.state.len();  // Example fitness

    // Only serialize when needed (e.g., checkpointing)
    if generation % 100 == 0 {
        let source = offspring.to_source();
        // Save to file...
    }
}
```

---

## Optimization Checklist

Before deploying Symbios in production, verify:

- [ ] Rules are minimal and non-redundant
- [ ] Expressions are simplified where possible
- [ ] State capacity is set appropriately (`sys.max_capacity`)
- [ ] Derivation depth is bounded
- [ ] Stochastic rules use `set_seed` for reproducibility in testing
- [ ] Benchmarks pass performance regression tests (`cargo bench`)
- [ ] Memory usage is profiled for your use case
- [ ] Evolutionary loops operate on `System` objects (not `SourceGenotype`) in hot paths

---

## Expected Performance Ranges

Based on benchmarks and real-world usage:

| Use Case              | Modules  | Derivations/sec | Notes                |
|-----------------------|----------|-----------------|----------------------|
| Small (< 100 modules) | 10-100   | 100,000+        | Interactive rates    |
| Medium (1K modules)   | 1,000    | 10,000+         | Real-time generation |
| Large (10K modules)   | 10,000   | 1,000+          | Batch processing     |
| Huge (100K+ modules)  | 100,000+ | 100+            | Precompute offline   |

**Hardware:** Apple M1/M2, AMD Ryzen 5000+, Intel i5/i7 (modern CPUs).

---

## References

- [Criterion.rs Benchmarking](https://github.com/bheisler/criterion.rs)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [Structure-of-Arrays Benefits](https://en.wikipedia.org/wiki/AoS_and_SoA)
