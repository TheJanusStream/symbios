//! Source-text-first evolvable L-system.
//!
//! `SourceGenotype` wraps L-system source code and provides genetic operations
//! (mutation, crossover) that maintain the source as the single source of truth.
//! After each operation, the modified system is decompiled back to source text
//! with full fidelity (preserving parameter names, comments, and directives).

use super::System;
use crate::system::SystemError;
use crate::system::crossover::CrossoverConfig;
use crate::system::mutate::{MutationConfig, StructuralMutationConfig};
use rand::Rng;

/// A source-text-first evolvable L-system.
///
/// Wraps L-system source code and provides the Parse → Mutate → Reconstruct
/// loop as a single abstraction. The source text is always the single source
/// of truth; compiled `System` state is transient.
///
/// # Example
/// ```
/// use symbios::SourceGenotype;
/// use symbios::system::mutate::MutationConfig;
/// use rand::SeedableRng;
/// use rand_pcg::Pcg64;
///
/// let mut genotype = SourceGenotype::new("omega: F\nF -> F F".to_string());
/// let mut rng = Pcg64::seed_from_u64(42);
/// let config = MutationConfig::default();
/// genotype.mutate_with_rng(&mut rng, &config).unwrap();
/// assert!(genotype.to_system().is_ok());
/// ```
#[derive(Debug, Clone)]
pub struct SourceGenotype {
    source: String,
}

impl SourceGenotype {
    /// Creates a new `SourceGenotype` from L-system source code.
    pub fn new(source: String) -> Self {
        Self { source }
    }

    /// Returns the current source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns a mutable reference to the source code.
    pub fn source_mut(&mut self) -> &mut String {
        &mut self.source
    }

    /// Parses the source into a `System` for derivation.
    pub fn to_system(&self) -> Result<System, SystemError> {
        System::from_source(&self.source)
    }

    /// Parse → Mutate → Reconstruct in one call.
    pub fn mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &MutationConfig,
    ) -> Result<(), SystemError> {
        let mut system = System::from_source(&self.source)?;
        system.mutate_with_rng(rng, config);
        self.source = system.to_source();
        Ok(())
    }

    /// Parse → Structural Mutate → Reconstruct in one call.
    pub fn structural_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &StructuralMutationConfig,
    ) -> Result<(), SystemError> {
        let mut system = System::from_source(&self.source)?;
        system.structural_mutate_with_rng(rng, config);
        self.source = system.to_source();
        Ok(())
    }

    /// Parse both → Crossover → Reconstruct offspring source.
    pub fn crossover_with_rng<R: Rng>(
        &self,
        other: &Self,
        rng: &mut R,
        config: &CrossoverConfig,
    ) -> Result<Self, SystemError> {
        let system_a = System::from_source(&self.source)?;
        let system_b = System::from_source(&other.source)?;
        let offspring = system_a.crossover_with_rng(&system_b, rng, config)?;
        Ok(Self {
            source: offspring.to_source(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_pcg::Pcg64;

    #[test]
    fn test_mutate_produces_valid_source() {
        let mut genotype = SourceGenotype::new("omega: F\nF -> F [ + F ] F".to_string());
        let mut rng = Pcg64::seed_from_u64(42);
        let config = MutationConfig::default();
        genotype.mutate_with_rng(&mut rng, &config).unwrap();
        assert!(genotype.to_system().is_ok());
    }

    #[test]
    fn test_crossover_produces_valid_source() {
        let a = SourceGenotype::new("omega: A\nA -> A B".to_string());
        let b = SourceGenotype::new("omega: A\nA -> A A".to_string());
        let mut rng = Pcg64::seed_from_u64(42);
        let config = CrossoverConfig::default();
        let offspring = a.crossover_with_rng(&b, &mut rng, &config).unwrap();
        assert!(offspring.to_system().is_ok());
    }

    #[test]
    fn test_crossover_preserves_axiom() {
        let a = SourceGenotype::new("omega: A\nA -> A B".to_string());
        let b = SourceGenotype::new("omega: A\nA -> A A".to_string());
        let mut rng = Pcg64::seed_from_u64(42);
        let config = CrossoverConfig::default();
        let offspring = a.crossover_with_rng(&b, &mut rng, &config).unwrap();
        assert!(
            offspring.source().contains("omega:"),
            "Offspring source should contain axiom, got: {}",
            offspring.source()
        );
    }

    #[test]
    fn test_round_trip_preserves_param_names() {
        let source = "omega: A(1)\nA(x) : x > 0 -> A(x - 1) B".to_string();
        let genotype = SourceGenotype::new(source);
        let system = genotype.to_system().unwrap();
        let output = system.to_source();
        assert!(
            output.contains("A(x)"),
            "Expected param name 'x' preserved, got: {}",
            output
        );
    }

    #[test]
    fn test_round_trip_preserves_comments() {
        let source =
            "// My L-system\n#define n 5\nomega: A(n)\nA(x) : x > 0 -> A(x - 1) B".to_string();
        let genotype = SourceGenotype::new(source);
        let system = genotype.to_system().unwrap();
        let output = system.to_source();
        assert!(
            output.contains("// My L-system"),
            "Expected comment preserved, got: {}",
            output
        );
        assert!(
            output.contains("#define n 5"),
            "Expected #define preserved, got: {}",
            output
        );
    }

    #[test]
    fn test_define_before_omega() {
        let source = "#define len 2.0\nomega: F(len)\nF(x) -> F(x) F(x)".to_string();
        let genotype = SourceGenotype::new(source);
        let system = genotype.to_system().unwrap();
        let output = system.to_source();

        let define_pos = output.find("#define len");
        let omega_pos = output.find("omega:");
        assert!(
            define_pos.is_some() && omega_pos.is_some(),
            "Both #define and omega should be present in: {}",
            output
        );
        assert!(
            define_pos.unwrap() < omega_pos.unwrap(),
            "#define should appear before omega in: {}",
            output
        );
    }

    #[test]
    fn test_structural_mutate_produces_valid_source() {
        let mut genotype = SourceGenotype::new("omega: F\nF -> F [ + F ] F".to_string());
        let mut rng = Pcg64::seed_from_u64(42);
        let config = StructuralMutationConfig::default();
        genotype
            .structural_mutate_with_rng(&mut rng, &config)
            .unwrap();
        assert!(genotype.to_system().is_ok());
    }
}
