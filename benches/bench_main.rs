use criterion::{Criterion, black_box, criterion_group, criterion_main};
use symbios::System;

fn bench_exponential_growth(c: &mut Criterion) {
    let mut group = c.benchmark_group("Derivation");

    // Scenario: Algae explosion (doubling every step)
    // A -> A B
    // B -> A
    // This tests pure allocation and state push speed.

    group.bench_function("Algae (Depth 15)", |b| {
        b.iter(|| {
            let mut sys = System::new();
            sys.add_rule("A -> A B").unwrap();
            sys.add_rule("B -> A").unwrap();
            sys.set_axiom("A").unwrap();
            // 15 generations = ~987 modules (Fibonacci)
            sys.derive(black_box(15)).unwrap();
        })
    });
    group.finish();
}

fn bench_context_heavy(c: &mut Criterion) {
    let mut group = c.benchmark_group("Context Matching");

    // Scenario: Signal Propagation
    // A(x) < B(y) > C(z) -> B(x+y+z)
    // This tests the O(1) topology lookups and O(N) rule matching overhead.

    group.bench_function("Signal Propagation (1000 modules)", |b| {
        b.iter_batched(
            || {
                let mut sys = System::new();
                sys.add_rule("A(x) < B(y) > C(z) -> B(x+y+z)").unwrap();

                // Construct a long chain: A(1) B(1) C(1) A(1) B(1) C(1)...
                // Length 3000
                let axiom = "A(1) B(1) C(1) ".repeat(1000);
                sys.set_axiom(&axiom).unwrap();

                // Calculate topology once (setup cost)
                let open = sys.interner.get_or_intern("[").unwrap();
                let close = sys.interner.get_or_intern("]").unwrap();
                sys.state.calculate_topology(open, close).unwrap();

                sys
            },
            |mut sys| {
                // Measure one derivation step
                sys.derive(black_box(1)).unwrap();
            },
            criterion::BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(benches, bench_exponential_growth, bench_context_heavy);
criterion_main!(benches);
