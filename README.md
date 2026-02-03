# Symbios

**A Sovereign Derivation Engine for Parametric L-Systems.**

Symbios is a pure-Rust, high-performance engine for generating Lindenmayer Systems. It is designed for "Sovereign" applications where the logic must run locally, deterministically, and safely (e.g., WASM environments, embedded simulation).

It fully implements the syntax and semantics described in *The Algorithmic Beauty of Plants* (Prusinkiewicz & Lindenmayer, 1990).

## Key Features

*   **Structure-of-Arrays (SoA)**: Data layout optimized for cache locality and WASM memory limits.
*   **Parametric & Context-Sensitive**: Full support for `(k,l)-systems`, arithmetic guards `A(x) : x > 5 -> ...`, and variable binding.
*   **Adversarial Hardening**: Protected against recursion bombs, memory exhaustion, and floating-point fragility.
*   **Deterministic**: Seedable RNG (`rand_pcg`) ensures reproducible procedural generation.

## Usage

```toml
[dependencies]
symbios = "1.2"
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

## Genetic Algorithm Support

Symbios includes built-in support for evolutionary optimization of L-Systems through mutation and crossover operations.

### Mutation

```rust
use symbios::{System, system::{MutationConfig, StructuralMutationConfig}};

let mut sys = System::new();
sys.add_rule("0.5: A -> A A").unwrap();
sys.add_rule("0.5: A -> B").unwrap();
sys.add_directive("#define ANGLE 45").unwrap();

// Mutate rule probabilities and constants
let config = MutationConfig {
    rule_probability_rate: 0.3,    // Chance to mutate each rule's probability
    rule_probability_strength: 0.1, // Max change to probabilities
    constant_rate: 0.2,            // Chance to mutate each constant
    constant_strength: 0.3,        // Relative change range for constants
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

### Crossover

```rust
use symbios::{System, system::CrossoverConfig};

let mut parent_a = System::new();
parent_a.add_rule("A -> A A").unwrap();
parent_a.add_directive("#define ANGLE 30").unwrap();

let mut parent_b = System::new();
parent_b.add_rule("A -> B").unwrap();
parent_b.add_directive("#define ANGLE 60").unwrap();

let config = CrossoverConfig {
    rule_bias: 0.5,       // Probability of taking rules from parent A vs B
    constant_blend: 0.5,  // Blending factor (0.0 = A, 1.0 = B, 0.5 = average)
};

let offspring = parent_a.crossover(&parent_b, &config);
// offspring.constants["ANGLE"] == 45.0 (blended)
```

### Reproducibility

All mutation and crossover operations have `_with_rng` variants that accept an external RNG for deterministic results:

```rust
use rand::SeedableRng;
use rand_pcg::Pcg64;

let mut rng = Pcg64::seed_from_u64(12345);
sys.mutate_with_rng(&mut rng, &config);
sys.structural_mutate_with_rng(&mut rng, &structural_config);
let offspring = parent_a.crossover_with_rng(&parent_b, &mut rng, &crossover_config);
```

## Performance

Symbios uses a flat memory arena for parameters and `u16` symbol interning.
*   **Rule Matching**: $O(N)$ (HashMap bucketed)
*   **Context Matching**: $O(1)$ (Topology Skip-Links)

See [PERFORMANCE.md](PERFORMANCE.md) for detailed benchmarks and optimization tips.

## Documentation

- **[ARCHITECTURE.md](ARCHITECTURE.md)** - System design, SoA layout, VM architecture, and design decisions
- **[PERFORMANCE.md](PERFORMANCE.md)** - Benchmark results, optimization tips, and profiling guide
- **[TROUBLESHOOTING.md](TROUBLESHOOTING.md)** - Common errors, solutions, and debugging patterns

## Examples

See [examples/](examples/) for complete working examples:
- [anabaena.rs](examples/anabaena.rs) - Simple discrete L-System from ABOP
- [monopodial_tree.rs](examples/monopodial_tree.rs) - Complex tree with branches and constants
- [stochastic_decay.rs](examples/stochastic_decay.rs) - Stochastic rule demonstration
- [adaptive_plant.rs](examples/adaptive_plant.rs) - Advanced example with age, context, and environment

## License

MIT
