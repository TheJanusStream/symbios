use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use symbios::System;
use symbios::system::{CrossoverConfig, MutationConfig, StructuralMutationConfig};

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

fn bench_mutation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Genetic/Mutation");

    group.bench_function("mutate (10 rules, 5 constants)", |b| {
        b.iter_batched(
            || {
                let mut sys = System::new();
                for i in 0..10 {
                    sys.add_rule(&format!("0.5: R{i} -> R{i} R{i}")).unwrap();
                }
                for i in 0..5 {
                    sys.add_directive(&format!("#define C{i} {}", i as f64 * 10.0))
                        .unwrap();
                }
                sys
            },
            |mut sys| {
                let config = MutationConfig {
                    rule_probability_rate: 0.5,
                    rule_probability_strength: 0.2,
                    constant_rate: 0.5,
                    constant_strength: 0.3,
                };
                sys.mutate(black_box(&config));
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("mutate (100 rules)", |b| {
        b.iter_batched(
            || {
                let mut sys = System::new();
                for i in 0..100 {
                    sys.add_rule(&format!("0.5: R{i} -> R{i}")).unwrap();
                }
                sys
            },
            |mut sys| {
                let config = MutationConfig::default();
                sys.mutate(black_box(&config));
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_crossover(c: &mut Criterion) {
    let mut group = c.benchmark_group("Genetic/Crossover");

    group.bench_function("crossover (10 rules each)", |b| {
        b.iter_batched(
            || {
                let mut parent_a = System::new();
                let mut parent_b = System::new();
                for i in 0..10 {
                    parent_a.add_rule(&format!("A{i} -> A{i} B{i}")).unwrap();
                    parent_b.add_rule(&format!("B{i} -> B{i} A{i}")).unwrap();
                }
                parent_a.add_directive("#define X 10").unwrap();
                parent_b.add_directive("#define X 90").unwrap();
                parent_a.add_directive("#define Y 20").unwrap();
                parent_b.add_directive("#define Z 30").unwrap();
                (parent_a, parent_b)
            },
            |(mut parent_a, parent_b)| {
                let config = CrossoverConfig::default();
                black_box(parent_a.crossover(&parent_b, &config).unwrap())
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("crossover (50 rules, 20 constants)", |b| {
        b.iter_batched(
            || {
                let mut parent_a = System::new();
                let mut parent_b = System::new();
                for i in 0..50 {
                    parent_a.add_rule(&format!("R{i} -> R{i}")).unwrap();
                    parent_b.add_rule(&format!("R{i} -> R{i} R{i}")).unwrap();
                }
                for i in 0..20 {
                    parent_a
                        .add_directive(&format!("#define C{i} {}", i as f64))
                        .unwrap();
                    parent_b
                        .add_directive(&format!("#define C{i} {}", i as f64 * 2.0))
                        .unwrap();
                }
                (parent_a, parent_b)
            },
            |(mut parent_a, parent_b)| {
                let config = CrossoverConfig::default();
                black_box(parent_a.crossover(&parent_b, &config).unwrap())
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_structural_mutation(c: &mut Criterion) {
    let mut group = c.benchmark_group("Genetic/StructuralMutation");

    group.bench_function("structural_mutate (10 rules, 5 successors each)", |b| {
        b.iter_batched(
            || {
                let mut sys = System::new();
                for i in 0..10 {
                    sys.add_rule(&format!("R{i} -> A B C D E")).unwrap();
                }
                sys
            },
            |mut sys| {
                let config = StructuralMutationConfig {
                    successor_rate: 0.5,
                    swap_rate: 0.3,
                    insert_rate: 0.2,
                    delete_rate: 0.1,
                    bytecode_rate: 0.0,
                    op_rate: 0.0,
                    push_perturbation: 0.0,
                };
                sys.structural_mutate(black_box(&config));
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.bench_function("structural_mutate with bytecode (parametric)", |b| {
        b.iter_batched(
            || {
                let mut sys = System::new();
                for i in 0..10 {
                    sys.add_rule(&format!("R{i}(x) -> R{i}(x+1) R{i}(x*2) R{i}(x-1)"))
                        .unwrap();
                }
                sys
            },
            |mut sys| {
                let config = StructuralMutationConfig {
                    successor_rate: 0.5,
                    swap_rate: 0.2,
                    insert_rate: 0.1,
                    delete_rate: 0.1,
                    bytecode_rate: 0.5,
                    op_rate: 0.3,
                    push_perturbation: 1.0,
                };
                sys.structural_mutate(black_box(&config));
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

fn bench_combined_evolution(c: &mut Criterion) {
    let mut group = c.benchmark_group("Genetic/CombinedEvolution");

    group.bench_function("full evolution cycle", |b| {
        b.iter_batched(
            || {
                let mut parent_a = System::new();
                let mut parent_b = System::new();
                for i in 0..20 {
                    parent_a
                        .add_rule(&format!("R{i}(x) -> R{i}(x+1) S{i}"))
                        .unwrap();
                    parent_b.add_rule(&format!("R{i}(x) -> R{i}(x*2)")).unwrap();
                }
                parent_a.add_directive("#define ANGLE 30").unwrap();
                parent_b.add_directive("#define ANGLE 60").unwrap();
                (parent_a, parent_b)
            },
            |(mut parent_a, parent_b)| {
                // Crossover
                let crossover_config = CrossoverConfig::default();
                let mut offspring = parent_a.crossover(&parent_b, &crossover_config).unwrap();

                // Mutation
                let mutation_config = MutationConfig::default();
                offspring.mutate(&mutation_config);

                // Structural mutation
                let structural_config = StructuralMutationConfig::default();
                offspring.structural_mutate(&structural_config);

                black_box(offspring)
            },
            criterion::BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_exponential_growth,
    bench_context_heavy,
    bench_mutation,
    bench_crossover,
    bench_structural_mutation,
    bench_combined_evolution
);
criterion_main!(benches);
