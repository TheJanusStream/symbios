# Symbios

**A Sovereign Derivation Engine for Parametric L-Systems.**

Symbios is a pure-Rust, zero-dependency (core), high-performance engine for generating Lindenmayer Systems. It is designed for "Sovereign" applications where the logic must run locally, deterministically, and safely (e.g., WASM environments, embedded simulation).

It fully implements the syntax and semantics described in *The Algorithmic Beauty of Plants* (Prusinkiewicz & Lindenmayer, 1990).

## Key Features

*   **Sovereign Architecture**: Zero external dependencies for the core logic. No game engines, no heavy runtimes.
*   **Structure-of-Arrays (SoA)**: Data layout optimized for cache locality and WASM memory limits.
*   **Parametric & Context-Sensitive**: Full support for `(k,l)-systems`, arithmetic guards `A(x) : x > 5 -> ...`, and variable binding.
*   **Adversarial Hardening**: Protected against recursion bombs, memory exhaustion, and floating-point fragility.
*   **Deterministic**: Seedable RNG (`rand_pcg`) ensures reproducible procedural generation.

## Usage

```toml
[dependencies]
symbios = "1.0.0"
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
