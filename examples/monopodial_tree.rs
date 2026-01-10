use symbios::System;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sys = System::new();

    // ABOP Figure 2.6 (p. 56)
    // Constants
    sys.add_directive("#define r1 0.9")?;
    sys.add_directive("#define r2 0.6")?;
    sys.add_directive("#define a0 45")?;
    sys.add_directive("#define a2 45")?;
    sys.add_directive("#define d 137.5")?;
    sys.add_directive("#define wr 0.707")?;

    // Rules
    // p1: A(l,w) -> !(w) F(l) [&(a0) B(l*r2, w*wr)] /(d) A(l*r1, w*wr)
    sys.add_rule("A(l,w) -> !(w) F(l) [&(a0) B(l*r1, w*wr)] /(d) A(l*r1, w*wr)")?;

    // p2: B(l,w) -> !(w) F(l) [-(a2) $ C(l*r2, w*wr)] C(l*r1, w*wr)
    sys.add_rule("B(l,w) -> !(w) F(l) [-(a2) $ C(l*r2, w*wr)] C(l*r1, w*wr)")?;

    // p3: C(l,w) -> !(w) F(l) [+(a2) $ B(l*r2, w*wr)] B(l*r1, w*wr)
    sys.add_rule("C(l,w) -> !(w) F(l) [+(a2) $ B(l*r2, w*wr)] B(l*r1, w*wr)")?;

    // Axiom: A(1, 10)
    sys.set_axiom("A(1, 10)")?;

    println!("--- Monopodial Tree (Honda) ---");
    println!("Axiom: {}", sys.state.display(&sys.interner));

    let iterations = 3;
    sys.derive(iterations)?;

    println!("State after {} derivations:", iterations);
    // Just printing length because the string is huge
    println!("Module count: {}", sys.state.len());

    // Demonstrate inspection
    let view = sys.state.get_view(0).unwrap();
    let sym = sys.interner.resolve(view.sym).unwrap();
    println!("Root module: {}{:?}", sym, view.params);

    Ok(())
}
