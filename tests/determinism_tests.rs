//! A seeded operation repeats exactly, whatever order the rule and constant
//! maps of the systems involved happen to iterate in.
//!
//! Parsing one source twice gives two systems whose `HashMap`s each have
//! their own random state, so they iterate in different orders. Before
//! operators walked rules and constants in a fixed order, the same seed
//! applied to the two gave different offspring and mutants.

use rand::SeedableRng;
use rand_pcg::Pcg64;
use symbios::System;
use symbios::system::crossover::{AdvancedCrossoverConfig, CrossoverConfig, CrossoverStrategy};
use symbios::system::mutate::{
    LiteralPromotionConfig, MutationConfig, OperatorFlipConfig, RuleDuplicationConfig,
    StructuralMutationConfig, TopologicalMutationConfig,
};

/// Enough rules and constants that two hash orders almost surely differ.
const PARENT_A: &str = "\
#define a 1.0
#define b 2.0
#define c 3.0
#define d 4.0
#define e 5.0
omega: A(a) B(b) C(c) D(d) E(e)
A(x) -> A(x * a) + B(x)
B(x) -> C(x + b) - [ D(x) ]
C(x) -> D(x * c) & E(x)
D(x) -> A(x + d) ^ [ B(x) ]
E(x) -> B(x - e) / C(x)
F -> G
G -> H
H -> F";

const PARENT_B: &str = "\
#define a 1.5
#define b 2.5
#define f 6.0
omega: A(a) C(b)
A(x) -> B(x) + A(x * f)
C(x) -> C(x - a) [ E(x) ]
E(x) -> A(x * b)
G -> F
H -> G";

const SEEDS: u64 = 16;

fn parse(source: &str) -> System {
    System::from_source(source).expect("test source parses")
}

#[test]
fn the_same_system_always_exports_the_same_source() {
    assert_eq!(parse(PARENT_A).to_source(), parse(PARENT_A).to_source());
}

#[test]
fn seeded_parametric_mutation_repeats() {
    let config = MutationConfig {
        rule_probability_rate: 0.5,
        rule_probability_strength: 0.2,
        constant_rate: 0.5,
        constant_strength: 0.3,
        gaussian_jitter_scale: 0.3,
        gaussian_jitter_rate: 0.5,
    };
    for seed in 0..SEEDS {
        let (mut x, mut y) = (parse(PARENT_A), parse(PARENT_A));
        x.mutate_with_rng(&mut Pcg64::seed_from_u64(seed), &config);
        y.mutate_with_rng(&mut Pcg64::seed_from_u64(seed), &config);
        assert_eq!(x.to_source(), y.to_source(), "seed {seed}");
    }
}

#[test]
fn seeded_structural_and_topological_mutation_repeats() {
    let structural = StructuralMutationConfig {
        successor_rate: 0.5,
        insert_rate: 0.3,
        delete_rate: 0.2,
        swap_rate: 0.3,
        bytecode_rate: 0.3,
        op_rate: 0.2,
        push_perturbation: 0.3,
    };
    let flip = OperatorFlipConfig {
        arithmetic_flip_rate: 0.3,
        relational_flip_rate: 0.3,
    };
    let duplication = RuleDuplicationConfig {
        duplication_rate: 0.3,
        condition_perturbation: 0.1,
    };
    let topological = TopologicalMutationConfig { swap_rate: 0.3 };
    for seed in 0..SEEDS {
        let (mut x, mut y) = (parse(PARENT_A), parse(PARENT_A));
        for sys in [&mut x, &mut y] {
            let mut rng = Pcg64::seed_from_u64(seed);
            sys.structural_mutate_with_rng(&mut rng, &structural);
            sys.operator_flip_mutate_with_rng(&mut rng, &flip);
            sys.rule_duplication_mutate_with_rng(&mut rng, &duplication);
            sys.topological_mutate_with_rng(&mut rng, &topological);
        }
        assert_eq!(x.to_source(), y.to_source(), "seed {seed}");
    }
}

#[test]
fn seeded_literal_promotion_repeats() {
    // Several constants within tolerance of one literal: the promoted name
    // must not depend on the constants map's order.
    const SOURCE: &str = "\
#define p 1.0
#define q 1.1
#define r 0.9
#define s 1.05
omega: A(1.0)
A(x) -> A(1.0) B(1.0)
B(x) -> A(1.02)";
    let config = LiteralPromotionConfig {
        promotion_rate: 1.0,
        match_tolerance: 0.5,
    };
    for seed in 0..SEEDS {
        let (mut x, mut y) = (parse(SOURCE), parse(SOURCE));
        x.literal_to_constant_promote_with_rng(&mut Pcg64::seed_from_u64(seed), &config);
        y.literal_to_constant_promote_with_rng(&mut Pcg64::seed_from_u64(seed), &config);
        assert_eq!(x.to_source(), y.to_source(), "seed {seed}");
    }
}

#[test]
fn seeded_homologous_crossover_repeats() {
    let config = AdvancedCrossoverConfig {
        strategy: CrossoverStrategy::Homologous,
        ..AdvancedCrossoverConfig::default()
    };
    for seed in 0..SEEDS {
        let child = |a: System, b: System| {
            a.advanced_crossover_with_rng(&b, &mut Pcg64::seed_from_u64(seed), &config)
                .expect("crossover succeeds")
                .to_source()
        };
        let x = child(parse(PARENT_A), parse(PARENT_B));
        let y = child(parse(PARENT_A), parse(PARENT_B));
        assert_eq!(x, y, "seed {seed}");
    }
}

#[test]
fn seeded_crossover_repeats() {
    let config = CrossoverConfig::default();
    for seed in 0..SEEDS {
        let child = |a: System, b: System| {
            a.crossover_with_rng(&b, &mut Pcg64::seed_from_u64(seed), &config)
                .expect("crossover succeeds")
                .to_source()
        };
        let x = child(parse(PARENT_A), parse(PARENT_B));
        let y = child(parse(PARENT_A), parse(PARENT_B));
        assert_eq!(x, y, "seed {seed}");
    }
}
