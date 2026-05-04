use rand::Rng;

use super::{RuntimeRule, System, SystemError, matching};

impl System {
    /// Advances the system by `steps` generations.
    ///
    /// This method:
    /// 1. Calculates topology (if brackets are present).
    /// 2. Iterates through the current state.
    /// 3. Matches rules (including context and guards).
    /// 4. Generates the new state.
    ///
    /// # Stochastic Rule Selection
    ///
    /// When multiple rules match a module, selection uses *relative weights*:
    /// - All matching rules' probabilities are summed to `total_weight`
    /// - Each rule is selected with probability `rule.probability / total_weight`
    /// - A single matching rule always fires (even with probability < 1.0)
    ///
    /// To implement probabilistic identity (e.g., "30% chance to transform"),
    /// define an explicit identity rule: `0.7: A -> A` alongside `0.3: A -> B`.
    pub fn derive(&mut self, steps: usize) -> Result<(), SystemError> {
        // Guard against non-finite current_time (e.g. set directly via pub field).
        // A non-finite time would propagate NaN through all age calculations,
        // permanently bricking the state.
        if !self.state.current_time.is_finite() {
            return Err(SystemError::State(
                crate::core::SymbiosError::InvalidNumericValue,
            ));
        }

        let mut vm = crate::vm::VirtualMachine::new();
        let open_sym = self.interner.resolve_id("[");
        let close_sym = self.interner.resolve_id("]");

        for _ in 0..steps {
            if let (Some(o), Some(c)) = (open_sym, close_sym) {
                self.state.calculate_topology(o, c)?;
            }

            // Prepare back buffer: clear contents but keep capacity
            self.back_buffer.clear();
            self.back_buffer.max_capacity = self.max_capacity;
            self.back_buffer.current_time = self.state.current_time;

            for index in 0..self.state.len() {
                let view = self
                    .state
                    .get_view(index)
                    .ok_or(crate::core::SymbiosError::InvalidIndex(index))?;

                // Track the last matched rule index to enable scratch buffer reuse
                let mut last_matched_idx: Option<usize> = None;

                let bucket = self.rules.get(&view.sym);

                // Select rule, branching on bucket size. Issue #92: most real
                // grammars have many symbols with exactly one rule. The
                // single-rule fast path skips the candidate buffer entirely,
                // saving the push/clear/loop overhead per module.
                let (selected_rule, selected_idx): (Option<&RuntimeRule>, Option<usize>) =
                    match bucket {
                        Some(b) if b.len() == 1 => {
                            // Fast path: a single rule. Skip the candidate
                            // buffer, but still honor the slow path's
                            // `total_probability <= 0.0` filter — explicit
                            // probability 0.0 suppresses even a sole matching
                            // rule (see test_explicit_probability_not_overwritten
                            // _by_numeric_condition in hardening_coverage_tests).
                            let rule = &b[0];
                            if rule.probability <= 0.0 {
                                (None, None)
                            } else {
                                // Issue #95: per-rule ignore list overrides global.
                                let ignored: &[u16] = rule
                                    .ignored_symbols
                                    .as_deref()
                                    .unwrap_or(&self.ignored_symbols);
                                let is_match = matching::matches(
                                    &self.state,
                                    index,
                                    rule,
                                    ignored,
                                    &mut vm,
                                    &mut self.scratch,
                                )?;
                                if is_match {
                                    last_matched_idx = Some(0);
                                    (Some(rule), Some(0))
                                } else {
                                    (None, None)
                                }
                            }
                        }
                        Some(b) => {
                            // Slow path: 0 or 2+ rules. Populate the candidate
                            // buffer, then weight-sample.
                            self.derive_candidate_indices.clear();
                            let mut total_probability = 0.0;
                            for (rule_idx, rule) in b.iter().enumerate() {
                                // view.sym is guaranteed to match rule.predecessor here
                                // Issue #95: per-rule ignore list overrides global.
                                let ignored: &[u16] = rule
                                    .ignored_symbols
                                    .as_deref()
                                    .unwrap_or(&self.ignored_symbols);
                                let is_match = matching::matches(
                                    &self.state,
                                    index,
                                    rule,
                                    ignored,
                                    &mut vm,
                                    &mut self.scratch,
                                )?;

                                if is_match {
                                    self.derive_candidate_indices.push(rule_idx);
                                    total_probability += rule.probability;
                                    last_matched_idx = Some(rule_idx);
                                } else {
                                    // scratch was cleared by matches() even though it failed;
                                    // invalidate so we don't reuse stale/empty data.
                                    last_matched_idx = None;
                                }
                            }

                            if self.derive_candidate_indices.is_empty() || total_probability <= 0.0
                            {
                                (None, None)
                            } else if self.derive_candidate_indices.len() == 1 {
                                let idx = self.derive_candidate_indices[0];
                                (b.get(idx), Some(idx))
                            } else {
                                // Sample directly off `total_probability` rather than
                                // floor-clamping to MIN_POSITIVE. Issue #91: the prior
                                // `random_range(0.0..safe_total)` distorted weight
                                // ratios when total_probability was subnormal — most
                                // r values landed above the per-rule weights and the
                                // last-candidate fallback won almost every time.
                                // `random::<f64>() * total_probability` preserves
                                // ratios for any total_probability > 0 (the early
                                // return above filters out the zero/negative case),
                                // and panics nowhere because it never divides by or
                                // ranges over zero.
                                let mut r = self.rng.random::<f64>() * total_probability;
                                let mut winner = None;
                                let mut winner_idx = None;
                                let last_candidate = self.derive_candidate_indices.len() - 1;
                                for (i, &rule_idx) in
                                    self.derive_candidate_indices.iter().enumerate()
                                {
                                    if let Some(rule) = b.get(rule_idx) {
                                        // Last candidate always wins to absorb any
                                        // floating-point residual, eliminating bias.
                                        if r < rule.probability || i == last_candidate {
                                            winner = Some(rule);
                                            winner_idx = Some(rule_idx);
                                            break;
                                        }
                                        r -= rule.probability;
                                    }
                                }
                                (winner, winner_idx)
                            }
                        }
                        None => (None, None),
                    };

                // Check if we can reuse scratch indices (selected rule was last matched)
                let can_reuse_scratch = selected_idx == last_matched_idx;

                if let Some(rule) = selected_rule {
                    // Clear and reuse generation buffers
                    self.gen_context_frame.clear();
                    self.gen_context_frame.extend_from_slice(view.params);

                    // Issue #95: per-rule ignore list overrides global. The
                    // post-selection context match must use the same list as
                    // the matching::matches() call did, so scratch reuse and
                    // re-matching agree on which symbols to skip.
                    let rule_ignored: &[u16] = rule
                        .ignored_symbols
                        .as_deref()
                        .unwrap_or(&self.ignored_symbols);

                    if !rule.left_context.is_empty() {
                        // Optimization: reuse scratch indices if selected rule was last matched,
                        // avoiding redundant context matching (O(L) per module savings)
                        let left_indices: &[usize] = if can_reuse_scratch {
                            &self.scratch.left_indices
                        } else {
                            self.gen_left_indices.clear();
                            matching::match_left(
                                &self.state,
                                index,
                                &rule.left_context,
                                rule_ignored,
                                &mut self.gen_left_indices,
                            );
                            &self.gen_left_indices
                        };
                        for &i in left_indices {
                            let ctx_view = self
                                .state
                                .get_view(i)
                                .ok_or(SystemError::StateCorruption(i))?;
                            self.gen_context_frame.extend_from_slice(ctx_view.params);
                        }
                    }

                    if !rule.right_context.is_empty() {
                        // Optimization: reuse scratch indices if selected rule was last matched
                        let right_indices: &[usize] = if can_reuse_scratch {
                            &self.scratch.right_indices
                        } else {
                            self.gen_right_indices.clear();
                            matching::match_right(
                                &self.state,
                                index,
                                &rule.right_context,
                                rule_ignored,
                                &mut self.gen_right_indices,
                            );
                            &self.gen_right_indices
                        };
                        for &i in right_indices {
                            let ctx_view = self
                                .state
                                .get_view(i)
                                .ok_or(SystemError::StateCorruption(i))?;
                            self.gen_context_frame.extend_from_slice(ctx_view.params);
                        }
                    }

                    for successor in &rule.successors {
                        self.gen_new_params.clear();
                        for param_code in &successor.params {
                            let val = vm
                                .eval(param_code, &self.gen_context_frame, view.age)
                                .map_err(SystemError::VMError)?;
                            self.gen_new_params.push(val);
                        }
                        self.back_buffer
                            .push(successor.symbol, 0.0, &self.gen_new_params)?;
                    }
                } else {
                    // Identity rule
                    self.back_buffer.push(view.sym, view.age, view.params)?;
                }
            }
            // Swap buffers: back_buffer becomes the new state,
            // current state becomes the recycled back_buffer for next step.
            std::mem::swap(&mut self.state, &mut self.back_buffer);
        }
        Ok(())
    }
}
