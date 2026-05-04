use super::{MAX_SUCCESSORS, RuntimeModule, RuntimeRule, System, SystemError};

use crate::vm::Op;
use rand::{Rng, SeedableRng};
use rand_pcg::Pcg64;
use std::collections::{HashMap, HashSet};

/// Configuration for crossover operations.
#[derive(Debug, Clone)]
pub struct CrossoverConfig {
    /// Probability of taking each rule from parent A vs parent B (0.5 = uniform).
    pub rule_bias: f64,
    /// Blending factor for constants (0.0 = parent A, 1.0 = parent B, 0.5 = average).
    pub constant_blend: f64,
}

impl Default for CrossoverConfig {
    fn default() -> Self {
        Self {
            rule_bias: 0.5,
            constant_blend: 0.5,
        }
    }
}

/// Strategy for rule crossover selection.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum CrossoverStrategy {
    /// Uniform random selection of entire rule sets per predecessor.
    #[default]
    Uniform,
    /// Homologous crossover: select subsets of rules per shared predecessor.
    Homologous,
}

/// Extended configuration for advanced crossover operations.
#[derive(Debug, Clone)]
pub struct AdvancedCrossoverConfig {
    /// Base crossover configuration.
    pub base: CrossoverConfig,
    /// Crossover strategy to use.
    pub strategy: CrossoverStrategy,
    /// For homologous crossover: probability of taking each individual rule from parent A.
    pub homologous_rule_bias: f64,
    /// BLX-alpha parameter for parametric blend crossover.
    /// When > 0, attempts to blend structurally identical rules by interpolating literals.
    pub blx_alpha: f64,
}

impl Default for AdvancedCrossoverConfig {
    fn default() -> Self {
        Self {
            base: CrossoverConfig::default(),
            strategy: CrossoverStrategy::Uniform,
            homologous_rule_bias: 0.5,
            blx_alpha: 0.0,
        }
    }
}

impl System {
    /// Performs crossover between this system and another, producing an offspring.
    ///
    /// Rules are selected from either parent based on the bias parameter.
    /// Constants are blended between parents. The offspring inherits the interner
    /// state needed to support all inherited rules.
    ///
    /// # Arguments
    /// * `other` - The other parent system
    /// * `config` - Controls crossover behavior
    ///
    /// # Returns
    /// A new `System` combining genetic material from both parents, or an error
    /// if the offspring's interner cannot accommodate all symbols.
    ///
    /// # Example
    /// ```
    /// use symbios::{System, system::crossover::CrossoverConfig};
    ///
    /// let mut parent_a = System::new();
    /// parent_a.add_rule("A -> A A").unwrap();
    ///
    /// let mut parent_b = System::new();
    /// parent_b.add_rule("A -> B").unwrap();
    ///
    /// let config = CrossoverConfig::default();
    /// let offspring = parent_a.crossover(&parent_b, &config).unwrap();
    /// ```
    pub fn crossover(
        &mut self,
        other: &System,
        config: &CrossoverConfig,
    ) -> Result<System, SystemError> {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        let result = self.crossover_with_rng(other, &mut rng, config);
        self.rng = rng;
        result
    }

    /// Performs crossover using an external RNG for reproducibility.
    ///
    /// # Errors
    /// Returns an error if the offspring's interner cannot accommodate all symbols
    /// from both parents (e.g., due to ID overflow or heap limits).
    pub fn crossover_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        config: &CrossoverConfig,
    ) -> Result<System, SystemError> {
        let mut offspring = System::new();
        offspring.rng = Pcg64::seed_from_u64(rng.random());
        offspring.max_capacity = self.max_capacity.max(other.max_capacity);

        let mut symbol_map_self: HashMap<u16, u16> = HashMap::new();
        let mut symbol_map_other: HashMap<u16, u16> = HashMap::new();

        for (old_id, name) in self.interner.iter() {
            let new_id = offspring
                .interner
                .get_or_intern(name)
                .map_err(SystemError::InternerError)?;
            symbol_map_self.insert(old_id, new_id);
        }

        for (old_id, name) in other.interner.iter() {
            let new_id = offspring
                .interner
                .get_or_intern(name)
                .map_err(SystemError::InternerError)?;
            symbol_map_other.insert(old_id, new_id);
        }

        // Use HashSet for O(1) lookup instead of O(N) Vec::contains
        let mut all_predecessors: HashSet<u16> = HashSet::new();
        for &pred in self.rules.keys() {
            if let Some(&new_pred) = symbol_map_self.get(&pred) {
                all_predecessors.insert(new_pred);
            }
        }
        for &pred in other.rules.keys() {
            if let Some(&new_pred) = symbol_map_other.get(&pred) {
                all_predecessors.insert(new_pred);
            }
        }

        // Build reverse mappings (new_id -> old_id) for O(1) lookup instead of O(N) scan
        let reverse_map_self: Vec<Option<u16>> = {
            let max_new_id = symbol_map_self.values().max().copied().unwrap_or(0) as usize;
            let mut reverse = vec![None; max_new_id + 1];
            for (&old_id, &new_id) in &symbol_map_self {
                reverse[new_id as usize] = Some(old_id);
            }
            reverse
        };
        let reverse_map_other: Vec<Option<u16>> = {
            let max_new_id = symbol_map_other.values().max().copied().unwrap_or(0) as usize;
            let mut reverse = vec![None; max_new_id + 1];
            for (&old_id, &new_id) in &symbol_map_other {
                reverse[new_id as usize] = Some(old_id);
            }
            reverse
        };

        // Track which parent contributed rules for each symbol (for consistent arity inheritance)
        let mut rules_from_self: HashSet<u16> = HashSet::new();
        let mut rules_from_other: HashSet<u16> = HashSet::new();

        for new_pred in all_predecessors {
            let self_pred = reverse_map_self.get(new_pred as usize).copied().flatten();
            let other_pred = reverse_map_other.get(new_pred as usize).copied().flatten();

            let self_rules = self_pred.and_then(|p| self.rules.get(&p));
            let other_rules = other_pred.and_then(|p| other.rules.get(&p));

            let (selected_rules, symbol_map, from_self) = match (self_rules, other_rules) {
                (Some(rules), None) => (rules, &symbol_map_self, true),
                (None, Some(rules)) => (rules, &symbol_map_other, false),
                (Some(self_r), Some(other_r)) => {
                    if rng.random::<f64>() < config.rule_bias {
                        (self_r, &symbol_map_self, true)
                    } else {
                        (other_r, &symbol_map_other, false)
                    }
                }
                (None, None) => continue,
            };

            // Track rule source for arity consistency
            if from_self {
                rules_from_self.insert(new_pred);
            } else {
                rules_from_other.insert(new_pred);
            }

            let mut remapped_rules: Vec<RuntimeRule> = Vec::new();
            for rule in selected_rules {
                let remapped = RuntimeRule {
                    predecessor: *symbol_map
                        .get(&rule.predecessor)
                        .unwrap_or(&rule.predecessor),
                    left_context: rule
                        .left_context
                        .iter()
                        .map(|s| *symbol_map.get(s).unwrap_or(s))
                        .collect(),
                    right_context: rule
                        .right_context
                        .iter()
                        .map(|s| *symbol_map.get(s).unwrap_or(s))
                        .collect(),
                    probability: rule.probability,
                    condition: rule.condition.clone(),
                    successors: rule
                        .successors
                        .iter()
                        .map(|s| RuntimeModule {
                            symbol: *symbol_map.get(&s.symbol).unwrap_or(&s.symbol),
                            params: s.params.clone(),
                        })
                        .collect(),
                    expected_arities: rule.expected_arities.clone(),
                    param_names: rule.param_names.clone(),
                    ignored_symbols: rule.ignored_symbols.as_ref().map(|ids| {
                        ids.iter()
                            .map(|s| *symbol_map.get(s).unwrap_or(s))
                            .collect()
                    }),
                };
                remapped_rules.push(remapped);
            }

            offspring.rules.insert(new_pred, remapped_rules);
        }

        let mut all_constant_keys: Vec<String> = self.constants.keys().cloned().collect();
        for key in other.constants.keys() {
            if !all_constant_keys.contains(key) {
                all_constant_keys.push(key.clone());
            }
        }

        for key in all_constant_keys {
            let val_a = self.constants.get(&key).copied();
            let val_b = other.constants.get(&key).copied();

            let blended = match (val_a, val_b) {
                (Some(a), Some(b)) => a * (1.0 - config.constant_blend) + b * config.constant_blend,
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => continue,
            };
            offspring.constants.insert(key, blended);
        }

        let mut new_ignored = Vec::new();

        for s in &self.ignored_symbols {
            if let Some(new_id) = symbol_map_self.get(s)
                && !new_ignored.contains(new_id)
            {
                new_ignored.push(*new_id);
            }
        }

        for s in &other.ignored_symbols {
            if let Some(new_id) = symbol_map_other.get(s)
                && !new_ignored.contains(new_id)
            {
                new_ignored.push(*new_id);
            }
        }

        offspring.ignored_symbols = new_ignored;

        // Inherit axiom and preamble from parent A for source round-tripping
        offspring.axiom_source = self.axiom_source.clone();
        offspring.preamble = self.preamble.clone();

        // Merge symbol_arities from both parents with rule-consistent inheritance.
        // For symbols with rules, use arity from the parent whose rules were selected.
        // This prevents arity mismatches where structural_mutate inserts modules
        // with parameter counts that don't match inherited rules.
        for (&old_id, &arity) in &self.symbol_arities {
            if let Some(&new_id) = symbol_map_self.get(&old_id) {
                // For symbols with rules from self, always use self's arity
                // For other symbols, insert only if not already present
                if rules_from_self.contains(&new_id) {
                    offspring.symbol_arities.insert(new_id, arity);
                } else {
                    offspring.symbol_arities.entry(new_id).or_insert(arity);
                }
            }
        }
        for (&old_id, &arity) in &other.symbol_arities {
            if let Some(&new_id) = symbol_map_other.get(&old_id) {
                // For symbols with rules from other, always use other's arity
                // This overrides any arity from self for these symbols
                if rules_from_other.contains(&new_id) {
                    offspring.symbol_arities.insert(new_id, arity);
                } else {
                    offspring.symbol_arities.entry(new_id).or_insert(arity);
                }
            }
        }

        Ok(offspring)
    }

    /// Performs advanced crossover with configurable strategy (Issues #50, #51).
    pub fn advanced_crossover(
        &mut self,
        other: &System,
        config: &AdvancedCrossoverConfig,
    ) -> Result<System, SystemError> {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        let result = self.advanced_crossover_with_rng(other, &mut rng, config);
        self.rng = rng;
        result
    }

    /// Performs advanced crossover with external RNG for reproducibility.
    pub fn advanced_crossover_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        config: &AdvancedCrossoverConfig,
    ) -> Result<System, SystemError> {
        match config.strategy {
            CrossoverStrategy::Uniform => self.crossover_with_rng(other, rng, &config.base),
            CrossoverStrategy::Homologous => self.homologous_crossover_with_rng(other, rng, config),
        }
    }

    /// Performs homologous rule crossover (Issue #50).
    ///
    /// Groups rules by predecessor symbol and selects individual rules from
    /// each parent rather than entire rule sets.
    fn homologous_crossover_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        config: &AdvancedCrossoverConfig,
    ) -> Result<System, SystemError> {
        let mut offspring = System::new();
        offspring.rng = Pcg64::seed_from_u64(rng.random());
        offspring.max_capacity = self.max_capacity.max(other.max_capacity);

        // Build symbol mappings
        let mut symbol_map_self: HashMap<u16, u16> = HashMap::new();
        let mut symbol_map_other: HashMap<u16, u16> = HashMap::new();

        for (old_id, name) in self.interner.iter() {
            let new_id = offspring
                .interner
                .get_or_intern(name)
                .map_err(SystemError::InternerError)?;
            symbol_map_self.insert(old_id, new_id);
        }

        for (old_id, name) in other.interner.iter() {
            let new_id = offspring
                .interner
                .get_or_intern(name)
                .map_err(SystemError::InternerError)?;
            symbol_map_other.insert(old_id, new_id);
        }

        // Collect all predecessor symbols
        let mut all_predecessors: HashSet<u16> = HashSet::new();
        for &pred in self.rules.keys() {
            if let Some(&new_pred) = symbol_map_self.get(&pred) {
                all_predecessors.insert(new_pred);
            }
        }
        for &pred in other.rules.keys() {
            if let Some(&new_pred) = symbol_map_other.get(&pred) {
                all_predecessors.insert(new_pred);
            }
        }

        // Build reverse mappings
        let reverse_map_self: HashMap<u16, u16> = symbol_map_self
            .iter()
            .map(|(&old, &new)| (new, old))
            .collect();
        let reverse_map_other: HashMap<u16, u16> = symbol_map_other
            .iter()
            .map(|(&old, &new)| (new, old))
            .collect();

        // Homologous crossover: select individual rules from each parent
        for new_pred in all_predecessors {
            let self_pred = reverse_map_self.get(&new_pred).copied();
            let other_pred = reverse_map_other.get(&new_pred).copied();

            let self_rules = self_pred.and_then(|p| self.rules.get(&p));
            let other_rules = other_pred.and_then(|p| other.rules.get(&p));

            let mut selected_rules: Vec<RuntimeRule> = Vec::new();

            // Select rules from self
            if let Some(rules) = self_rules {
                for rule in rules {
                    if rng.random::<f64>() < config.homologous_rule_bias {
                        let remapped = Self::remap_rule(rule, &symbol_map_self);
                        // Try BLX-alpha blend if enabled and matching rule exists in other
                        if config.blx_alpha > 0.0
                            && let Some(other_rules) = other_rules
                            && let Some(blended) = Self::try_blx_alpha_blend(
                                &remapped,
                                other_rules,
                                &symbol_map_other,
                                config.blx_alpha,
                                rng,
                            )
                        {
                            selected_rules.push(blended);
                            continue;
                        }
                        selected_rules.push(remapped);
                    }
                }
            }

            // Select rules from other
            if let Some(rules) = other_rules {
                for rule in rules {
                    if rng.random::<f64>() >= config.homologous_rule_bias {
                        let remapped = Self::remap_rule(rule, &symbol_map_other);
                        selected_rules.push(remapped);
                    }
                }
            }

            if !selected_rules.is_empty() {
                offspring.rules.insert(new_pred, selected_rules);
            }
        }

        // Blend constants
        let mut all_constant_keys: Vec<String> = self.constants.keys().cloned().collect();
        for key in other.constants.keys() {
            if !all_constant_keys.contains(key) {
                all_constant_keys.push(key.clone());
            }
        }

        for key in all_constant_keys {
            let val_a = self.constants.get(&key).copied();
            let val_b = other.constants.get(&key).copied();

            let blended = match (val_a, val_b) {
                (Some(a), Some(b)) => {
                    a * (1.0 - config.base.constant_blend) + b * config.base.constant_blend
                }
                (Some(a), None) => a,
                (None, Some(b)) => b,
                (None, None) => continue,
            };
            offspring.constants.insert(key, blended);
        }

        // Merge ignored symbols
        let mut new_ignored = Vec::new();
        for s in &self.ignored_symbols {
            if let Some(new_id) = symbol_map_self.get(s)
                && !new_ignored.contains(new_id)
            {
                new_ignored.push(*new_id);
            }
        }
        for s in &other.ignored_symbols {
            if let Some(new_id) = symbol_map_other.get(s)
                && !new_ignored.contains(new_id)
            {
                new_ignored.push(*new_id);
            }
        }
        offspring.ignored_symbols = new_ignored;

        // Inherit axiom and preamble from parent A for source round-tripping
        offspring.axiom_source = self.axiom_source.clone();
        offspring.preamble = self.preamble.clone();

        // Merge symbol arities
        for (&old_id, &arity) in &self.symbol_arities {
            if let Some(&new_id) = symbol_map_self.get(&old_id) {
                offspring.symbol_arities.entry(new_id).or_insert(arity);
            }
        }
        for (&old_id, &arity) in &other.symbol_arities {
            if let Some(&new_id) = symbol_map_other.get(&old_id) {
                offspring.symbol_arities.entry(new_id).or_insert(arity);
            }
        }

        Ok(offspring)
    }

    fn remap_rule(rule: &RuntimeRule, symbol_map: &HashMap<u16, u16>) -> RuntimeRule {
        RuntimeRule {
            predecessor: *symbol_map
                .get(&rule.predecessor)
                .unwrap_or(&rule.predecessor),
            left_context: rule
                .left_context
                .iter()
                .map(|s| *symbol_map.get(s).unwrap_or(s))
                .collect(),
            right_context: rule
                .right_context
                .iter()
                .map(|s| *symbol_map.get(s).unwrap_or(s))
                .collect(),
            probability: rule.probability,
            condition: rule.condition.clone(),
            successors: rule
                .successors
                .iter()
                .map(|s| RuntimeModule {
                    symbol: *symbol_map.get(&s.symbol).unwrap_or(&s.symbol),
                    params: s.params.clone(),
                })
                .collect(),
            expected_arities: rule.expected_arities.clone(),
            param_names: rule.param_names.clone(),
            ignored_symbols: rule.ignored_symbols.as_ref().map(|ids| {
                ids.iter()
                    .map(|s| *symbol_map.get(s).unwrap_or(s))
                    .collect()
            }),
        }
    }

    /// Attempts BLX-alpha blending between structurally identical rules (Issue #51).
    ///
    /// If a rule in `other_rules` has the same bytecode structure (same ops, different
    /// Push values), interpolates the literals using BLX-alpha.
    fn try_blx_alpha_blend<R: Rng>(
        rule_a: &RuntimeRule,
        other_rules: &[RuntimeRule],
        symbol_map: &HashMap<u16, u16>,
        alpha: f64,
        rng: &mut R,
    ) -> Option<RuntimeRule> {
        for rule_b in other_rules {
            // Check structural compatibility
            if !Self::rules_structurally_compatible(rule_a, rule_b, symbol_map) {
                continue;
            }

            // Found a compatible rule, perform BLX-alpha blend
            let mut blended = rule_a.clone();
            blended.probability = (rule_a.probability + rule_b.probability) / 2.0;

            // Blend condition bytecode
            if let (Some(cond_a), Some(cond_b)) =
                (blended.condition.as_mut(), rule_b.condition.as_ref())
            {
                Self::blx_alpha_blend_bytecode(cond_a, cond_b, alpha, rng);
            }

            // Blend successor parameters
            for (succ_a, succ_b) in blended.successors.iter_mut().zip(&rule_b.successors) {
                for (params_a, params_b) in succ_a.params.iter_mut().zip(&succ_b.params) {
                    Self::blx_alpha_blend_bytecode(params_a, params_b, alpha, rng);
                }
            }

            return Some(blended);
        }
        None
    }

    fn rules_structurally_compatible(
        rule_a: &RuntimeRule,
        rule_b: &RuntimeRule,
        symbol_map: &HashMap<u16, u16>,
    ) -> bool {
        // Check predecessor matches (after remapping)
        let remapped_pred = symbol_map
            .get(&rule_b.predecessor)
            .unwrap_or(&rule_b.predecessor);
        if rule_a.predecessor != *remapped_pred {
            return false;
        }

        // Check successor counts match
        if rule_a.successors.len() != rule_b.successors.len() {
            return false;
        }

        // Check each successor has compatible structure
        for (succ_a, succ_b) in rule_a.successors.iter().zip(&rule_b.successors) {
            let remapped_sym = symbol_map.get(&succ_b.symbol).unwrap_or(&succ_b.symbol);
            if succ_a.symbol != *remapped_sym {
                return false;
            }
            if succ_a.params.len() != succ_b.params.len() {
                return false;
            }
            // Check bytecode structure (same ops, ignoring Push values)
            for (params_a, params_b) in succ_a.params.iter().zip(&succ_b.params) {
                if !Self::bytecode_structurally_equal(params_a, params_b) {
                    return false;
                }
            }
        }

        true
    }

    fn bytecode_structurally_equal(a: &[Op], b: &[Op]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        for (op_a, op_b) in a.iter().zip(b) {
            let same_structure = match (op_a, op_b) {
                (Op::Push(_), Op::Push(_)) => true,
                (Op::LoadParam(i), Op::LoadParam(j)) => i == j,
                (Op::LoadAge, Op::LoadAge) => true,
                (Op::Add, Op::Add) => true,
                (Op::Sub, Op::Sub) => true,
                (Op::Mul, Op::Mul) => true,
                (Op::Div, Op::Div) => true,
                (Op::Pow, Op::Pow) => true,
                (Op::Neg, Op::Neg) => true,
                (Op::Eq, Op::Eq) => true,
                (Op::Ne, Op::Ne) => true,
                (Op::Gt, Op::Gt) => true,
                (Op::Lt, Op::Lt) => true,
                (Op::Ge, Op::Ge) => true,
                (Op::Le, Op::Le) => true,
                (Op::And, Op::And) => true,
                (Op::Or, Op::Or) => true,
                (Op::Not, Op::Not) => true,
                (Op::Math(m1), Op::Math(m2)) => m1 == m2,
                _ => false,
            };
            if !same_structure {
                return false;
            }
        }
        true
    }

    /// Performs BLX-alpha blending on bytecode Push values.
    fn blx_alpha_blend_bytecode<R: Rng>(a: &mut [Op], b: &[Op], alpha: f64, rng: &mut R) {
        for (op_a, op_b) in a.iter_mut().zip(b) {
            let (val_a, val_b) = match (op_a as &Op, op_b) {
                (Op::Push(va), Op::Push(vb)) => (*va, *vb),
                _ => continue,
            };

            let min_val = val_a.min(val_b);
            let max_val = val_a.max(val_b);
            let range = max_val - min_val;

            // BLX-alpha: sample from [min - alpha*range, max + alpha*range]
            let low = min_val - alpha * range;
            let high = max_val + alpha * range;

            let blended = if (high - low).abs() < f64::EPSILON {
                val_a
            } else {
                rng.random_range(low..=high)
            };

            if blended.is_finite() {
                *op_a = Op::Push(blended);
            }
        }
    }

    /// Performs sub-expression grafting crossover (Issue #52).
    ///
    /// Swaps branch blocks ([...]) between parent successor sequences.
    pub fn subexpression_graft(
        &mut self,
        other: &System,
        graft_rate: f64,
    ) -> Result<System, SystemError> {
        let mut rng = std::mem::replace(&mut self.rng, Pcg64::seed_from_u64(0));
        let result = self.subexpression_graft_with_rng(other, &mut rng, graft_rate);
        self.rng = rng;
        result
    }

    /// Performs sub-expression grafting with external RNG for reproducibility.
    pub fn subexpression_graft_with_rng<R: Rng>(
        &self,
        other: &System,
        rng: &mut R,
        graft_rate: f64,
    ) -> Result<System, SystemError> {
        // Start with standard crossover
        let config = CrossoverConfig::default();
        let mut offspring = self.crossover_with_rng(other, rng, &config)?;

        // Get bracket symbols
        let open_sym = offspring.interner.resolve_id("[");
        let close_sym = offspring.interner.resolve_id("]");

        if open_sym.is_none() || close_sym.is_none() {
            return Ok(offspring);
        }

        let open = open_sym.unwrap();
        let close = close_sym.unwrap();

        // Extract branch blocks from other parent's rules
        let other_branches: Vec<Vec<RuntimeModule>> = other
            .rules
            .values()
            .flat_map(|rules| rules.iter())
            .flat_map(|rule| Self::extract_branch_blocks(&rule.successors, open, close))
            .collect();

        if other_branches.is_empty() {
            return Ok(offspring);
        }

        // Graft branches into offspring rules
        for rules in offspring.rules.values_mut() {
            for rule in rules.iter_mut() {
                if rng.random::<f64>() < graft_rate && !other_branches.is_empty() {
                    // Find a branch in current rule to replace
                    if let Some((start, end)) =
                        Self::find_branch_block(&rule.successors, open, close)
                    {
                        // Select a random branch from other parent
                        let donor_idx = rng.random_range(0..other_branches.len());
                        let donor_branch = &other_branches[donor_idx];

                        // Replace the branch
                        let mut new_successors = Vec::new();
                        new_successors.extend_from_slice(&rule.successors[..start]);
                        new_successors.extend(donor_branch.iter().cloned());
                        new_successors.extend_from_slice(&rule.successors[end + 1..]);

                        // Enforce MAX_SUCCESSORS limit
                        if new_successors.len() <= MAX_SUCCESSORS {
                            rule.successors = new_successors;
                        }
                    }
                }
            }
        }

        Ok(offspring)
    }

    /// Extracts all balanced branch blocks from a successor sequence.
    fn extract_branch_blocks(
        successors: &[RuntimeModule],
        open: u16,
        close: u16,
    ) -> Vec<Vec<RuntimeModule>> {
        let mut blocks = Vec::new();
        let mut i = 0;

        while i < successors.len() {
            if successors[i].symbol == open
                && let Some(end) = Self::find_matching_close(successors, i, open, close)
            {
                blocks.push(successors[i..=end].to_vec());
                i = end + 1;
                continue;
            }
            i += 1;
        }

        blocks
    }

    /// Finds the first branch block in successors.
    fn find_branch_block(
        successors: &[RuntimeModule],
        open: u16,
        close: u16,
    ) -> Option<(usize, usize)> {
        for (i, module) in successors.iter().enumerate() {
            if module.symbol == open
                && let Some(end) = Self::find_matching_close(successors, i, open, close)
            {
                return Some((i, end));
            }
        }
        None
    }

    /// Finds the matching close bracket for an open bracket at position `start`.
    fn find_matching_close(
        successors: &[RuntimeModule],
        start: usize,
        open: u16,
        close: u16,
    ) -> Option<usize> {
        let mut depth = 0;
        for (i, module) in successors.iter().enumerate().skip(start) {
            if module.symbol == open {
                depth += 1;
            } else if module.symbol == close {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    }
}
