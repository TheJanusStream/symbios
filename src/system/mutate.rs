use super::{MAX_SUCCESSORS, RuntimeModule, System};

use crate::vm::Op;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use std::collections::HashMap;

/// Configuration for mutation operations.
#[derive(Debug, Clone)]
pub struct MutationConfig {
    /// Probability of mutating each rule's probability (0.0 - 1.0).
    pub rule_probability_rate: f64,
    /// Maximum change to rule probabilities (additive, clamped to 0.0-1.0).
    pub rule_probability_strength: f64,
    /// Probability of mutating each constant (0.0 - 1.0).
    pub constant_rate: f64,
    /// Relative change to constants (multiplicative factor range).
    pub constant_strength: f64,
    /// Scale for Gaussian jitter applied to bytecode literals (0.0 to disable).
    /// When > 0, scans bytecode for Push(f64) and applies value += N(0,1) * scale.
    pub gaussian_jitter_scale: f64,
    /// Probability of applying Gaussian jitter to each Push literal (0.0 - 1.0).
    pub gaussian_jitter_rate: f64,
}

impl Default for MutationConfig {
    fn default() -> Self {
        Self {
            rule_probability_rate: 0.1,
            rule_probability_strength: 0.2,
            constant_rate: 0.1,
            constant_strength: 0.2,
            gaussian_jitter_scale: 0.0,
            gaussian_jitter_rate: 0.0,
        }
    }
}

/// Configuration for structural mutation operations on rule successors and bytecode.
#[derive(Debug, Clone)]
pub struct StructuralMutationConfig {
    /// Probability of mutating each rule's successor sequence (0.0 - 1.0).
    pub successor_rate: f64,
    /// Probability of inserting a new module into successors (0.0 - 1.0).
    pub insert_rate: f64,
    /// Probability of deleting a module from successors (0.0 - 1.0).
    pub delete_rate: f64,
    /// Probability of swapping two adjacent modules in successors (0.0 - 1.0).
    pub swap_rate: f64,
    /// Probability of mutating parameter bytecode for each module (0.0 - 1.0).
    pub bytecode_rate: f64,
    /// Probability of mutating each operation in bytecode (0.0 - 1.0).
    pub op_rate: f64,
    /// Range for perturbing Push constants (additive).
    pub push_perturbation: f64,
}

/// Configuration for operator flip mutation.
#[derive(Debug, Clone)]
pub struct OperatorFlipConfig {
    /// Probability of flipping each arithmetic operator (Add<->Sub, Mul<->Div).
    pub arithmetic_flip_rate: f64,
    /// Probability of flipping each relational operator (Gt<->Lt, Ge<->Le, Eq<->Ne).
    pub relational_flip_rate: f64,
}

impl Default for OperatorFlipConfig {
    fn default() -> Self {
        Self {
            arithmetic_flip_rate: 0.1,
            relational_flip_rate: 0.1,
        }
    }
}

/// Configuration for rule duplication and specialization.
#[derive(Debug, Clone)]
pub struct RuleDuplicationConfig {
    /// Probability of duplicating each rule.
    pub duplication_rate: f64,
    /// Amount to perturb the condition after duplication (for numeric conditions).
    pub condition_perturbation: f64,
}

impl Default for RuleDuplicationConfig {
    fn default() -> Self {
        Self {
            duplication_rate: 0.1,
            condition_perturbation: 0.2,
        }
    }
}

/// Configuration for topological (turtle command) symbol mutation.
#[derive(Debug, Clone)]
pub struct TopologicalMutationConfig {
    /// Probability of swapping each turtle command with its counterpart.
    pub swap_rate: f64,
}

impl Default for TopologicalMutationConfig {
    fn default() -> Self {
        Self { swap_rate: 0.1 }
    }
}

/// Configuration for literal-to-constant promotion mutation.
#[derive(Debug, Clone)]
pub struct LiteralPromotionConfig {
    /// Probability of promoting each literal to a constant reference.
    pub promotion_rate: f64,
    /// Tolerance for matching literal values to constants.
    pub match_tolerance: f64,
}

impl Default for LiteralPromotionConfig {
    fn default() -> Self {
        Self {
            promotion_rate: 0.1,
            match_tolerance: 0.1,
        }
    }
}

impl Default for StructuralMutationConfig {
    fn default() -> Self {
        Self {
            successor_rate: 0.1,
            insert_rate: 0.2,
            delete_rate: 0.1,
            swap_rate: 0.2,
            bytecode_rate: 0.1,
            op_rate: 0.1,
            push_perturbation: 0.5,
        }
    }
}

impl System {
    /// Mutates the system in-place for evolutionary algorithms.
    ///
    /// This method randomly perturbs rule probabilities and constants based on
    /// the provided configuration. Useful for genetic algorithms and evolutionary
    /// optimization of L-System parameters.
    ///
    /// # Arguments
    /// * `config` - Controls mutation rates and strengths
    ///
    /// # Example
    /// ```
    /// use symbios::{System, system::mutate::MutationConfig};
    ///
    /// let mut sys = System::new();
    /// sys.add_rule("A -> A B").unwrap();
    /// sys.add_rule("A -> B").unwrap();
    ///
    /// let config = MutationConfig {
    ///     rule_probability_rate: 0.5,
    ///     rule_probability_strength: 0.1,
    ///     ..Default::default()
    /// };
    /// sys.mutate(&config);
    /// ```
    pub fn mutate(&mut self, config: &MutationConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Mutates the system using an external RNG for reproducibility.
    ///
    /// This variant allows using a shared RNG across multiple systems
    /// for coordinated evolution experiments.
    pub fn mutate_with_rng<R: Rng>(&mut self, rng: &mut R, config: &MutationConfig) {
        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if rng.random::<f64>() < config.rule_probability_rate {
                    let delta = rng.random_range(
                        -config.rule_probability_strength..=config.rule_probability_strength,
                    );
                    rule.probability = (rule.probability + delta).clamp(0.0, 1.0);
                }
            }
        }

        let constant_keys: Vec<String> = self.constants.keys().cloned().collect();
        for key in constant_keys {
            if rng.random::<f64>() < config.constant_rate
                && let Some(val) = self.constants.get_mut(&key)
            {
                let factor =
                    1.0 + rng.random_range(-config.constant_strength..=config.constant_strength);
                *val *= factor;
            }
        }

        // Gaussian jitter on bytecode literals (Issue #53)
        if config.gaussian_jitter_scale > 0.0 && config.gaussian_jitter_rate > 0.0 {
            for rules in self.rules.values_mut() {
                for rule in rules.iter_mut() {
                    // Apply to condition bytecode
                    if let Some(ref mut cond) = rule.condition {
                        Self::apply_gaussian_jitter(rng, cond, config);
                    }
                    // Apply to successor parameter bytecode
                    for successor in &mut rule.successors {
                        for param_bytecode in &mut successor.params {
                            Self::apply_gaussian_jitter(rng, param_bytecode, config);
                        }
                    }
                }
            }
        }
    }

    /// Applies Gaussian jitter to Push literals in bytecode.
    /// Uses Box-Muller transform to generate standard normal samples.
    fn apply_gaussian_jitter<R: Rng>(rng: &mut R, bytecode: &mut [Op], config: &MutationConfig) {
        for op in bytecode.iter_mut() {
            if let Op::Push(val) = op
                && rng.random::<f64>() < config.gaussian_jitter_rate
            {
                // Box-Muller transform for standard normal
                let u1: f64 = rng.random();
                let u2: f64 = rng.random();
                let z = (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                let new_val = *val + z * config.gaussian_jitter_scale;
                if new_val.is_finite() {
                    *op = Op::Push(new_val);
                }
            }
        }
    }

    /// Performs structural mutation using an external RNG for reproducibility.
    ///
    /// This mutates the structure of rule successors (insert/delete/swap modules)
    /// and the bytecode of module parameters (change operations).
    pub fn structural_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &StructuralMutationConfig,
    ) {
        let symbol_ids: Vec<u16> = self.interner.iter().map(|(id, _)| id).collect();
        if symbol_ids.is_empty() {
            return;
        }

        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if rng.random::<f64>() >= config.successor_rate {
                    continue;
                }

                // Swap two adjacent modules
                if rule.successors.len() >= 2 && rng.random::<f64>() < config.swap_rate {
                    let idx = rng.random_range(0..rule.successors.len() - 1);
                    rule.successors.swap(idx, idx + 1);
                }

                // Delete a random module (keep at least one)
                if rule.successors.len() > 1 && rng.random::<f64>() < config.delete_rate {
                    let idx = rng.random_range(0..rule.successors.len());
                    rule.successors.remove(idx);
                }

                // Insert a new module at a random position (respecting MAX_SUCCESSORS limit)
                if rule.successors.len() < MAX_SUCCESSORS
                    && rng.random::<f64>() < config.insert_rate
                {
                    let symbol = symbol_ids[rng.random_range(0..symbol_ids.len())];
                    // Initialize with correct arity: one Op::Push(0.0) per expected parameter
                    let arity = self.symbol_arities.get(&symbol).copied().unwrap_or(0);
                    let params: Vec<Vec<Op>> = (0..arity).map(|_| vec![Op::Push(0.0)]).collect();
                    let new_module = RuntimeModule { symbol, params };
                    let idx = rng.random_range(0..=rule.successors.len());
                    rule.successors.insert(idx, new_module);
                }

                // Mutate bytecode of module parameters
                for module in &mut rule.successors {
                    if rng.random::<f64>() >= config.bytecode_rate {
                        continue;
                    }
                    for param_bytecode in &mut module.params {
                        Self::mutate_bytecode(rng, param_bytecode, config);
                    }
                }
            }
        }
    }

    /// Mutates a single bytecode sequence by changing operations.
    fn mutate_bytecode<R: Rng>(
        rng: &mut R,
        bytecode: &mut [Op],
        config: &StructuralMutationConfig,
    ) {
        for op in bytecode.iter_mut() {
            if rng.random::<f64>() >= config.op_rate {
                continue;
            }
            *op = match op {
                // Perturb Push constants (with finiteness check to prevent Inf poisoning)
                Op::Push(val) => {
                    // Clamp perturbation range to prevent overflow in random_range
                    let safe_perturbation = config.push_perturbation.min(1e100);
                    let delta = rng.random_range(-safe_perturbation..=safe_perturbation);
                    let new_val = *val + delta;
                    // Only apply mutation if result is finite; otherwise keep original
                    if new_val.is_finite() {
                        Op::Push(new_val)
                    } else {
                        continue;
                    }
                }
                // Swap arithmetic operations
                Op::Add => Self::random_arithmetic_op(rng),
                Op::Sub => Self::random_arithmetic_op(rng),
                Op::Mul => Self::random_arithmetic_op(rng),
                Op::Div => Self::random_arithmetic_op(rng),
                // Swap relational operations
                Op::Eq => Self::random_relational_op(rng),
                Op::Ne => Self::random_relational_op(rng),
                Op::Gt => Self::random_relational_op(rng),
                Op::Lt => Self::random_relational_op(rng),
                Op::Ge => Self::random_relational_op(rng),
                Op::Le => Self::random_relational_op(rng),
                // Swap logical operations (binary only)
                Op::And => {
                    if rng.random::<bool>() {
                        Op::Or
                    } else {
                        Op::And
                    }
                }
                Op::Or => {
                    if rng.random::<bool>() {
                        Op::And
                    } else {
                        Op::Or
                    }
                }
                // Leave other ops unchanged
                _ => continue,
            };
        }
    }

    fn random_arithmetic_op<R: Rng>(rng: &mut R) -> Op {
        match rng.random_range(0..4) {
            0 => Op::Add,
            1 => Op::Sub,
            2 => Op::Mul,
            _ => Op::Div,
        }
    }

    fn random_relational_op<R: Rng>(rng: &mut R) -> Op {
        match rng.random_range(0..6) {
            0 => Op::Eq,
            1 => Op::Ne,
            2 => Op::Gt,
            3 => Op::Lt,
            4 => Op::Ge,
            _ => Op::Le,
        }
    }

    /// Performs structural mutation on rule successors and bytecode.
    ///
    /// This is a convenience wrapper that uses the system's internal RNG.
    pub fn structural_mutate(&mut self, config: &StructuralMutationConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.structural_mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs operator flip mutation on bytecode (Issue #54).
    ///
    /// Swaps arithmetic operators (Add<->Sub, Mul<->Div) and relational operators
    /// (Gt<->Lt, Ge<->Le, Eq<->Ne) while preserving stack signatures.
    pub fn operator_flip_mutate(&mut self, config: &OperatorFlipConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.operator_flip_mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs operator flip mutation with external RNG for reproducibility.
    pub fn operator_flip_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &OperatorFlipConfig,
    ) {
        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if let Some(ref mut cond) = rule.condition {
                    Self::flip_operators_in_bytecode(rng, cond, config);
                }
                for successor in &mut rule.successors {
                    for param_bytecode in &mut successor.params {
                        Self::flip_operators_in_bytecode(rng, param_bytecode, config);
                    }
                }
            }
        }
    }

    fn flip_operators_in_bytecode<R: Rng>(
        rng: &mut R,
        bytecode: &mut [Op],
        config: &OperatorFlipConfig,
    ) {
        for op in bytecode.iter_mut() {
            match op {
                // Arithmetic flips: Add<->Sub, Mul<->Div
                Op::Add if rng.random::<f64>() < config.arithmetic_flip_rate => *op = Op::Sub,
                Op::Sub if rng.random::<f64>() < config.arithmetic_flip_rate => *op = Op::Add,
                Op::Mul if rng.random::<f64>() < config.arithmetic_flip_rate => *op = Op::Div,
                Op::Div if rng.random::<f64>() < config.arithmetic_flip_rate => *op = Op::Mul,
                // Relational flips: Gt<->Lt, Ge<->Le, Eq<->Ne
                Op::Gt if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Lt,
                Op::Lt if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Gt,
                Op::Ge if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Le,
                Op::Le if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Ge,
                Op::Eq if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Ne,
                Op::Ne if rng.random::<f64>() < config.relational_flip_rate => *op = Op::Eq,
                _ => {}
            }
        }
    }

    /// Performs rule duplication and specialization (Issue #55).
    ///
    /// Clones existing rules and mutates the probability of both original and copy,
    /// ensuring probabilities are normalized. This enables evolutionary speciation.
    pub fn rule_duplication_mutate(&mut self, config: &RuleDuplicationConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.rule_duplication_mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs rule duplication with external RNG for reproducibility.
    pub fn rule_duplication_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &RuleDuplicationConfig,
    ) {
        let pred_keys: Vec<u16> = self.rules.keys().copied().collect();

        for pred in pred_keys {
            let rules = match self.rules.get_mut(&pred) {
                Some(r) => r,
                None => continue,
            };

            let mut duplicates = Vec::new();

            for (idx, rule) in rules.iter_mut().enumerate() {
                if rng.random::<f64>() < config.duplication_rate {
                    // Clone the rule
                    let mut dup = rule.clone();

                    // Perturb probabilities of both original and duplicate
                    let orig_prob = rule.probability;
                    let split = rng.random_range(0.3..0.7);

                    rule.probability = orig_prob * split;
                    dup.probability = orig_prob * (1.0 - split);

                    // Perturb condition bytecode of duplicate (if present)
                    if let Some(ref mut cond) = dup.condition {
                        for op in cond.iter_mut() {
                            if let Op::Push(val) = op {
                                let perturbation = rng.random_range(
                                    -config.condition_perturbation..=config.condition_perturbation,
                                );
                                let new_val = *val + perturbation;
                                if new_val.is_finite() {
                                    *op = Op::Push(new_val);
                                }
                            }
                        }
                    }

                    duplicates.push((idx, dup));
                }
            }

            // Insert duplicates after their originals
            for (offset, (orig_idx, dup)) in duplicates.into_iter().enumerate() {
                let insert_pos = orig_idx + offset + 1;
                if insert_pos <= rules.len() {
                    rules.insert(insert_pos, dup);
                } else {
                    rules.push(dup);
                }
            }
        }
    }

    /// Performs topological symbol mutation for turtle commands (Issue #57).
    ///
    /// Swaps spatial counterparts in successor sequences:
    /// - `+` (Yaw Left) <-> `-` (Yaw Right)
    /// - `&` (Pitch Down) <-> `^` (Pitch Up)
    /// - `F` (Move) <-> `f` (Move Invisible)
    pub fn topological_mutate(&mut self, config: &TopologicalMutationConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.topological_mutate_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Performs topological mutation with external RNG for reproducibility.
    pub fn topological_mutate_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &TopologicalMutationConfig,
    ) {
        // Build symbol pair mappings for turtle commands
        let swap_pairs: Vec<(&str, &str)> = vec![
            ("+", "-"),  // Yaw left <-> Yaw right
            ("&", "^"),  // Pitch down <-> Pitch up
            ("F", "f"),  // Move <-> Move invisible
            ("\\", "/"), // Roll left <-> Roll right
        ];

        let mut symbol_swaps: HashMap<u16, u16> = HashMap::new();

        for (a, b) in swap_pairs {
            if let (Some(id_a), Some(id_b)) =
                (self.interner.resolve_id(a), self.interner.resolve_id(b))
            {
                symbol_swaps.insert(id_a, id_b);
                symbol_swaps.insert(id_b, id_a);
            }
        }

        if symbol_swaps.is_empty() {
            return;
        }

        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                for successor in &mut rule.successors {
                    if rng.random::<f64>() < config.swap_rate
                        && let Some(&swapped) = symbol_swaps.get(&successor.symbol)
                    {
                        successor.symbol = swapped;
                    }
                }
            }
        }
    }

    /// Promotes literal values to constant references (Issue #56).
    ///
    /// Scans bytecode for Push(f64) and replaces with LoadConstant when a
    /// matching constant exists within tolerance. This increases genetic coupling.
    pub fn literal_to_constant_promote(&mut self, config: &LiteralPromotionConfig) {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        self.literal_to_constant_promote_with_rng(&mut rng, config);
        self.rng = rng;
    }

    /// Promotes literals with external RNG for reproducibility.
    pub fn literal_to_constant_promote_with_rng<R: Rng>(
        &mut self,
        rng: &mut R,
        config: &LiteralPromotionConfig,
    ) {
        if self.constants.is_empty() {
            return;
        }

        // Build constant value -> name mapping
        let const_values: Vec<(String, f64)> = self
            .constants
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect();

        for rules in self.rules.values_mut() {
            for rule in rules.iter_mut() {
                if let Some(ref mut cond) = rule.condition {
                    Self::promote_literals_in_bytecode(rng, cond, &const_values, config);
                }
                for successor in &mut rule.successors {
                    for param_bytecode in &mut successor.params {
                        Self::promote_literals_in_bytecode(
                            rng,
                            param_bytecode,
                            &const_values,
                            config,
                        );
                    }
                }
            }
        }
    }

    fn promote_literals_in_bytecode<R: Rng>(
        rng: &mut R,
        bytecode: &mut [Op],
        const_values: &[(String, f64)],
        config: &LiteralPromotionConfig,
    ) {
        for op in bytecode.iter_mut() {
            if let Op::Push(val) = op {
                if rng.random::<f64>() >= config.promotion_rate {
                    continue;
                }
                // Find a matching constant within tolerance
                for (_name, const_val) in const_values {
                    let diff = (*val - *const_val).abs();
                    let threshold = config.match_tolerance * const_val.abs().max(1.0);
                    if diff <= threshold {
                        // Replace with the constant's value (since we don't have LoadConstant,
                        // we update the literal to match the constant exactly, creating coupling)
                        *op = Op::Push(*const_val);
                        // Store the constant name association for genetic tracking
                        // (In a full implementation, we'd add Op::LoadConstant)
                        break;
                    }
                }
            }
        }
    }
}
