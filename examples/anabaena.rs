use symbios::System;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = System::new();

    // ABOP Eq 1.1: Anabaena catenula
    // ar -> al br
    // al -> bl ar
    // br -> ar
    // bl -> al

    sys.add_rule("ar -> al br")?;
    sys.add_rule("al -> bl ar")?;
    sys.add_rule("br -> ar")?;
    sys.add_rule("bl -> al")?;

    sys.set_axiom("ar")?;

    println!("--- Anabaena Development ---");
    println!("Gen 0: {}", sys.state.display(&sys.interner));

    for i in 1..=5 {
        sys.derive(1)?;
        println!("Gen {}: {}", i, sys.state.display(&sys.interner));
    }

    Ok(())
}
