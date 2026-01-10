use symbios::System;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = System::new();

    // Simulation of a radioactive isotope decay or cell death
    // A: Active Cell
    // D: Dead Cell

    // 50% chance to survive (A -> A)
    // 50% chance to die (A -> D)
    sys.add_rule("0.5 : A -> A")?;
    sys.add_rule("0.5 : A -> D")?;

    // Dead cells fade away (D -> epsilon)
    // We use an empty successor to delete the module
    sys.add_rule("D -> ")?;

    // Start with a population of 20 cells
    sys.set_axiom("A A A A A A A A A A A A A A A A A A A A")?;

    println!("--- Stochastic Decay Simulation ---");
    println!("T=0: {}", sys.state.display(&sys.interner));

    for i in 1..=5 {
        sys.derive(1)?;
        let count = sys.state.len();
        println!("T={}: {} cells remaining", i, count);
        // Optional: Print state if small enough
        if count < 50 {
            println!("     {}", sys.state.display(&sys.interner));
        }
    }

    Ok(())
}
