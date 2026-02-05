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

                // Reuse scratch buffer for candidate indices (avoids per-module allocation)
                self.derive_candidate_indices.clear();
                let mut total_probability = 0.0;
                // Track the last matched rule index to enable scratch buffer reuse
                let mut last_matched_idx: Option<usize> = None;

                if let Some(bucket) = self.rules.get(&view.sym) {
                    for (rule_idx, rule) in bucket.iter().enumerate() {
                        // view.sym is guaranteed to match rule.predecessor here
                        let is_match = matching::matches(
                            &self.state,
                            index,
                            rule,
                            &self.ignored_symbols,
                            &mut vm,
                            &mut self.scratch,
                        )?;

                        if is_match {
                            self.derive_candidate_indices.push(rule_idx);
                            total_probability += rule.probability;
                            last_matched_idx = Some(rule_idx);
                        }
                    }
                }

                // Select rule from candidates using indices, tracking which index was selected
                let (selected_rule, selected_idx): (Option<&RuntimeRule>, Option<usize>) =
                    if self.derive_candidate_indices.is_empty() || total_probability <= 0.0 {
                        (None, None)
                    } else if self.derive_candidate_indices.len() == 1 {
                        let idx = self.derive_candidate_indices[0];
                        (
                            self.rules.get(&view.sym).and_then(|b| b.get(idx)),
                            Some(idx),
                        )
                    } else {
                        let bucket = self.rules.get(&view.sym);
                        let mut r = self.rng.random_range(0.0..total_probability);
                        let mut winner = None;
                        let mut winner_idx = None;
                        for &rule_idx in &self.derive_candidate_indices {
                            if let Some(rule) = bucket.and_then(|b| b.get(rule_idx)) {
                                if r < rule.probability {
                                    winner = Some(rule);
                                    winner_idx = Some(rule_idx);
                                    break;
                                }
                                r -= rule.probability;
                            }
                        }
                        if winner.is_none() {
                            let fallback_idx = self.derive_candidate_indices.last().copied();
                            (
                                bucket.and_then(|b| fallback_idx.and_then(|i| b.get(i))),
                                fallback_idx,
                            )
                        } else {
                            (winner, winner_idx)
                        }
                    };

                // Check if we can reuse scratch indices (selected rule was last matched)
                let can_reuse_scratch = selected_idx == last_matched_idx;

                if let Some(rule) = selected_rule {
                    // Clear and reuse generation buffers
                    self.gen_context_frame.clear();
                    self.gen_context_frame.extend_from_slice(view.params);

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
                                &self.ignored_symbols,
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
                                &self.ignored_symbols,
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
