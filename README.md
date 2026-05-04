# Symbios

**A Sovereign Derivation Engine for Parametric L-Systems.**

Symbios is a pure-Rust, high-performance engine for generating Lindenmayer Systems. It is designed for "Sovereign" applications where the logic must run locally, deterministically, and safely (e.g., WASM environments, embedded simulation).

It fully implements the syntax and semantics described in *The Algorithmic Beauty of Plants* (Prusinkiewicz & Lindenmayer, 1990).

## Key Features

* **Structure-of-Arrays (SoA)**: Data layout optimized for cache locality and WASM memory limits.
* **Parametric & Context-Sensitive**: Full support for `(k,l)-systems`, arithmetic guards `A(x) : x > 5 -> ...`, and variable binding.
* **Genetic Algorithm Toolkit**: 9 mutation operators, 4 crossover strategies, and lossless source round-tripping for evolutionary optimization.
* **Adversarial Hardening**: Protected against recursion bombs, memory exhaustion, and floating-point fragility.
* **Deterministic**: Seedable RNG (`rand_pcg`) ensures reproducible procedural generation.
* **Zero `unsafe` Code**: All safety via Rust's type system and explicit bounds checks.

## Usage

```toml
[dependencies]
symbios = "1.4"
```

```rust
use symbios::System;

fn main() {
    let mut sys = System::new();

    // 1. Define Rules (ABOP Syntax)
    // A module 'A' with parameter 'x' grows if 'x' is small
    sys.add_rule("A(x) : x < 10 -> A(x + 1) B(x)").unwrap();

    // 2. Set Axiom
    sys.set_axiom("A(0)").unwrap();

    // 3. Derive
    sys.derive(5).unwrap();

    // 4. Inspect
    println!("{}", sys.state.display(&sys.interner));
}
```

### SourceGenotype (Evolvable Wrapper)

For evolutionary loops that operate at the source-text level:

```rust
use symbios::SourceGenotype;
use symbios::system::{MutationConfig, CrossoverConfig};
use rand::SeedableRng;
use rand_pcg::Pcg64;

let mut genotype = SourceGenotype::new("A -> A A\nomega: A".into());
let mut rng = Pcg64::seed_from_u64(42);

// Mutate in source space
genotype.mutate_with_rng(&mut rng, &MutationConfig::default()).unwrap();

// Crossover two genotypes
let other = SourceGenotype::new("A -> B\nomega: A".into());
let child = genotype.crossover_with_rng(&other, &mut rng, &CrossoverConfig::default()).unwrap();

// Realize into a runnable System
let sys = child.to_system().unwrap();
```

## Genetic Algorithm Support

Symbios includes a comprehensive toolkit for evolutionary optimization of L-Systems.

### Basic Mutation

```rust
use symbios::{System, system::{MutationConfig, StructuralMutationConfig}};

let mut sys = System::new();
sys.add_rule("0.5: A -> A A").unwrap();
sys.add_rule("0.5: A -> B").unwrap();
sys.add_directive("#define ANGLE 45").unwrap();

// Mutate rule probabilities and constants
let config = MutationConfig {
    rule_probability_rate: 0.3,        // Chance to mutate each rule's probability
    rule_probability_strength: 0.1,    // Max change to probabilities
    constant_rate: 0.2,                // Chance to mutate each constant
    constant_strength: 0.3,            // Relative change range for constants
    gaussian_jitter_scale: 0.0,        // Scale for Gaussian noise on bytecode literals
    gaussian_jitter_rate: 0.0,         // Chance to jitter each Push literal
};
sys.mutate(&config);

// Structural mutation: insert/delete/swap modules, perturb bytecode
let structural_config = StructuralMutationConfig {
    successor_rate: 0.2,      // Chance to mutate each rule's successors
    insert_rate: 0.1,         // Chance to insert a new module
    delete_rate: 0.1,         // Chance to delete a module
    swap_rate: 0.2,           // Chance to swap adjacent modules
    bytecode_rate: 0.1,       // Chance to mutate parameter bytecode
    op_rate: 0.1,             // Chance to change arithmetic operations
    push_perturbation: 0.5,   // Range for perturbing constants in bytecode
};
sys.structural_mutate(&structural_config);
```

### Advanced Mutation Operators

```rust
use symbios::system::{
    OperatorFlipConfig, RuleDuplicationConfig,
    TopologicalMutationConfig, LiteralPromotionConfig,
};

// Operator flip: swap semantically-paired operators (Add<->Sub, Gt<->Lt, etc.)
sys.operator_flip_mutate(&OperatorFlipConfig {
    arithmetic_flip_rate: 0.1,   // Add<->Sub, Mul<->Div
    relational_flip_rate: 0.1,   // Gt<->Lt, Ge<->Le, Eq<->Ne
});

// Rule duplication: clone a rule, split probability, perturb the copy
sys.rule_duplication_mutate(&RuleDuplicationConfig {
    duplication_rate: 0.1,
    condition_perturbation: 0.2,
});

// Topological mutation: swap turtle command pairs (+/-, &/^, F/f, \//)
sys.topological_mutate(&TopologicalMutationConfig {
    swap_rate: 0.1,
});

// Literal-to-constant promotion: replace Push literals with matching #define values
sys.literal_to_constant_promote(&LiteralPromotionConfig {
    promotion_rate: 0.1,
    match_tolerance: 0.1,
});
```

### Crossover

```rust
use symbios::{System, system::{CrossoverConfig, AdvancedCrossoverConfig, CrossoverStrategy}};

let mut parent_a = System::new();
parent_a.add_rule("A -> A A").unwrap();
parent_a.add_directive("#define ANGLE 30").unwrap();

let mut parent_b = System::new();
parent_b.add_rule("A -> B").unwrap();
parent_b.add_directive("#define ANGLE 60").unwrap();

// Basic crossover: select whole rule sets per symbol, blend constants
let basic_config = CrossoverConfig {
    rule_bias: 0.5,       // Probability of taking rules from parent A vs B
    constant_blend: 0.5,  // Blending factor (0.0 = A, 1.0 = B, 0.5 = average)
};
let offspring = parent_a.crossover(&parent_b, &basic_config).unwrap();

// Advanced crossover: homologous rule selection + BLX-alpha blending
let advanced_config = AdvancedCrossoverConfig {
    base: basic_config,
    strategy: CrossoverStrategy::Homologous,  // Or CrossoverStrategy::Uniform
    homologous_rule_bias: 0.5,                // Per-rule selection bias
    blx_alpha: 0.5,                           // BLX-alpha exploration range
};
let offspring = parent_a.advanced_crossover(&parent_b, &advanced_config).unwrap();

// Sub-expression grafting: swap balanced branch blocks [...]
let offspring = parent_a.subexpression_graft(&parent_b, 0.3).unwrap();
```

### Reproducibility

All mutation and crossover operations have `_with_rng` variants that accept an external RNG for deterministic results:

```rust
use rand::SeedableRng;
use rand_pcg::Pcg64;

let mut rng = Pcg64::seed_from_u64(12345);
sys.mutate_with_rng(&mut rng, &config);
sys.structural_mutate_with_rng(&mut rng, &structural_config);
sys.operator_flip_mutate_with_rng(&mut rng, &flip_config);
sys.rule_duplication_mutate_with_rng(&mut rng, &dup_config);
sys.topological_mutate_with_rng(&mut rng, &topo_config);
sys.literal_to_constant_promote_with_rng(&mut rng, &promo_config);
let offspring = parent_a.crossover_with_rng(&parent_b, &mut rng, &crossover_config);
let offspring = parent_a.advanced_crossover_with_rng(&parent_b, &mut rng, &advanced_config);
let offspring = parent_a.subexpression_graft_with_rng(&parent_b, &mut rng, 0.3);
```

## Rule Export

Symbios can decompile compiled rules back to source text. This is useful for inspecting mutated rules, serialization, and debugging.

### Export All Rules

```rust
use symbios::System;

let mut sys = System::new();
sys.add_rule("A(x) : x > 10 -> B(x + 1)").unwrap();
sys.add_rule("A(x) : x <= 10 -> A(x + 1)").unwrap();

// Export all rules as (predecessor, source) pairs
for (pred, source) in sys.export_rules() {
    println!("{}: {}", pred, source);
}
```

### Export Rules for a Specific Symbol

```rust
let rules = sys.export_rules_for("A");
for rule in rules {
    println!("{}", rule);
}
```

### Export a Specific Rule

```rust
let rule = sys.export_rule_at("A", 0).unwrap();
println!("{}", rule);
```

### Custom Parameter Names

By default, exported rules use synthetic parameter names (`p0`, `p1`, ...). You can provide custom names:

```rust
let rule = sys.export_rule_with_params("A", 0, vec!["age".into()]).unwrap();
println!("{}", rule);
// Output: A(age) : age > 10 -> B(age + 1)
```

## Directives

```rust
// Constants: define reusable numeric values
sys.add_directive("#define ANGLE 45")?;
sys.add_directive("#define GROWTH_RATE 1.2")?;

// Ignore: symbols to skip during context matching
sys.add_directive("#ignore: F f +")?;

// Per-rule override (issue #95): a rule can supply its own ignore list
// with a `{ ignore: ... }` postfix. The per-rule list fully replaces the
// global list for that rule only. Use an empty list to disable ignoring.
sys.add_rule("A < B > C -> X { ignore: + - }")?;  // shadow with custom list
sys.add_rule("A < B > C -> Y { ignore: }")?;       // shadow with empty list
```

## Stochastic Rules

Rules can have probability weights for stochastic selection:

```rust
// Probability prefix syntax
sys.add_rule("0.7: A -> A B")?;
sys.add_rule("0.3: A -> A")?;

// Set seed for deterministic results
sys.set_seed(42);
sys.derive(10)?;
```

When multiple rules match a module, one is selected with probability proportional to its weight. A single matching rule always fires regardless of its probability value.

## Temporal Dynamics

Modules track their age (time since creation). The `age` variable is available in conditions and expressions:

```rust
sys.add_rule("A(x) : age > 5 -> B(x)")?;
sys.set_axiom("A(1)")?;

// Advance time and derive
sys.state.advance_time(1.0)?;
sys.derive(1)?;
```

## Performance

Symbios uses a flat memory arena for parameters and `u16` symbol interning.

* **Rule Matching**: $O(N)$ (HashMap bucketed)
* **Context Matching**: $O(1)$ (Topology Skip-Links)

See [PERFORMANCE.md](PERFORMANCE.md) for detailed benchmarks and optimization tips.

## Documentation

* **[ARCHITECTURE.md](ARCHITECTURE.md)** - System design, SoA layout, VM architecture, and design decisions
* **[PERFORMANCE.md](PERFORMANCE.md)** - Benchmark results, optimization tips, and profiling guide
* **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** - Common errors, solutions, and debugging patterns

## Examples

See [examples/](examples/) for complete working examples:

* [anabaena.rs](examples/anabaena.rs) - Simple discrete L-System from ABOP
* [monopodial_tree.rs](examples/monopodial_tree.rs) - Complex tree with branches and constants
* [stochastic_decay.rs](examples/stochastic_decay.rs) - Stochastic rule demonstration
* [adaptive_plant.rs](examples/adaptive_plant.rs) - Advanced example with age, context, and environment

## License

MIT
