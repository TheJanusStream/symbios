use symbios::System;

/// Adaptive Plant Growth
///
/// This example demonstrates advanced Symbios features:
/// - Context-sensitive rules (A < B > C)
/// - Age-based growth (using `age` variable)
/// - Stochastic branching (using probability prefix: "0.3 : rule")
/// - Environmental parameters (light, nutrients)
/// - Temporal dynamics (time advancement)
/// - Constants and conditional logic
///
/// Model Description:
/// A plant grows from a meristem (M) that produces segments (S).
/// Segments age over time and may:
/// - Branch (stochastic, based on light availability)
/// - Produce leaves (L) when mature
/// - Die (D) when too old
///
/// Environmental factors:
/// - Light: High at the top (apex), decreases downward
/// - Nutrients: Shared resource, affects growth rate
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = System::new();
    sys.set_seed(42); // Deterministic stochastic behavior

    // === Constants ===
    sys.add_directive("#define MATURITY_AGE 3.0")?; // Age when segment can branch
    sys.add_directive("#define MAX_AGE 10.0")?; // Age when segment dies
    sys.add_directive("#define GROWTH_RATE 1.2")?; // Segment elongation factor
    sys.add_directive("#define LIGHT_DECAY 0.9")?; // Light reduction per segment
    sys.add_directive("#define MIN_LIGHT 0.1")?; // Minimum light for growth

    // === Rules ===

    // 1. Meristem Growth (apex produces segments)
    // M(light, nutrients) -> Segment + new Meristem
    // Only grows if sufficient light and nutrients
    sys.add_rule(
        "M(light, nut) : light > MIN_LIGHT & nut > 0.5 -> S(light, 0) M(light * GROWTH_RATE, nut * 0.9)"
    )?;

    // 2. Segment Maturation (age-based branching)
    // Mature segments may branch stochastically (30% probability)
    // Branching consumes light and nutrients
    // Note: Symbios uses probability prefix syntax: "0.3 : rule"
    sys.add_rule(
        "0.3 : S(light, age_param) : age >= MATURITY_AGE & age < MAX_AGE & light > 0.5 -> S(light, age) [ +(45) M(light * 0.7, 0.6) ] [ -(45) M(light * 0.7, 0.6) ]"
    )?;

    // 3. Leaf Production (context-sensitive)
    // Segments between meristem and other segments produce leaves
    // Only if they haven't already (no leaf L in context)
    sys.add_rule(
        "S(light, age_param) < M(l2, n2) : age >= MATURITY_AGE -> S(light, age) L(light)",
    )?;

    // 4. Segment Aging and Light Decay
    // Older segments reduce light availability
    // This rule applies to non-branching segments
    sys.add_rule("S(light, age_param) : age < MAX_AGE -> S(light * LIGHT_DECAY, age)")?;

    // 5. Death (segments too old)
    sys.add_rule("S(light, age_param) : age >= MAX_AGE -> D")?;

    // 6. Dead segments are terminal (no further growth)
    sys.add_rule("D -> D")?;

    // 7. Leaves persist
    sys.add_rule("L(light) -> L(light * LIGHT_DECAY)")?;

    // === Axiom ===
    // Start with a meristem at full light and nutrients
    sys.set_axiom("M(1.0, 1.0)")?;

    // === Simulation ===
    println!("=== Adaptive Plant Growth Simulation ===\n");
    println!("Initial state: {}\n", sys.state.display(&sys.interner));

    // Simulate growth over multiple time steps
    let time_steps = 6;
    for step in 1..=time_steps {
        // Advance time (increases all module ages)
        sys.state.advance_time(1.0)?;

        // Derive one generation
        sys.derive(1)?;

        println!("--- Step {} (age: {:.1}) ---", step, sys.state.current_time);
        println!("Module count: {}", sys.state.len());

        // Count module types
        let mut counts = std::collections::HashMap::new();
        for i in 0..sys.state.len() {
            if let Some(view) = sys.state.get_view(i) {
                let sym = sys.interner.resolve(view.sym).unwrap_or("?");
                *counts.entry(sym).or_insert(0) += 1;
            }
        }

        println!("Composition:");
        for (sym, count) in counts.iter() {
            println!("  {}: {}", sym, count);
        }

        // Show a sample segment's state
        for i in 0..sys.state.len() {
            if let Some(view) = sys.state.get_view(i) {
                let sym = sys.interner.resolve(view.sym).unwrap_or("?");
                if sym == "S" && view.params.len() >= 2 {
                    println!(
                        "  Sample segment: light={:.3}, age={:.1}",
                        view.params[0], view.age
                    );
                    break;
                }
            }
        }

        println!();

        // Safety check: prevent runaway growth
        if sys.state.len() > 1000 {
            println!("Growth limit reached, stopping simulation.");
            break;
        }
    }

    // === Final Analysis ===
    println!("=== Final State ===");
    println!("Total modules: {}", sys.state.len());
    println!("Final time: {:.1}", sys.state.current_time);

    // Export first 100 characters of string representation
    let output = sys.state.display(&sys.interner).to_string();
    let preview = if output.len() > 100 {
        format!("{}...", &output[..100])
    } else {
        output
    };
    println!("String (preview): {}", preview);

    println!("\n=== Features Demonstrated ===");
    println!("✓ Context-sensitive rules (leaf production)");
    println!("✓ Age-based growth (maturation, death)");
    println!("✓ Stochastic branching (probability prefix syntax)");
    println!("✓ Environmental parameters (light decay)");
    println!("✓ Temporal dynamics (time advancement)");
    println!("✓ Constants (reusable values)");
    println!("✓ Conditional logic (multiple conditions)");
    println!("✓ Branching structures (brackets)");

    Ok(())
}
