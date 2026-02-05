use super::{System, SystemError};

impl System {
    /// Exports all rules in the system to source text.
    ///
    /// Returns a vector of (predecessor_symbol, rule_source) pairs.
    /// Uses synthetic parameter names (p0, p1, ...) since original names
    /// are not preserved during compilation.
    ///
    /// # Example
    /// ```
    /// use symbios::System;
    ///
    /// let mut sys = System::new();
    /// sys.add_rule("A(x) : x > 10 -> B(x + 1)").unwrap();
    /// sys.add_rule("A(x) : x <= 10 -> A(x + 1)").unwrap();
    ///
    /// let exported = sys.export_rules();
    /// assert_eq!(exported.len(), 2);
    /// ```
    pub fn export_rules(&self) -> Vec<(String, String)> {
        let mut results = Vec::new();

        for rules in self.rules.values() {
            for rule in rules {
                let config = crate::export::ExportConfig::synthetic(rule);
                if let Ok(source) =
                    crate::export::export_rule_to_string(rule, &self.interner, &config)
                {
                    let pred_name = self
                        .interner
                        .resolve(rule.predecessor)
                        .unwrap_or("?")
                        .to_string();
                    results.push((pred_name, source));
                }
            }
        }

        results
    }

    /// Exports all rules for a specific predecessor symbol.
    ///
    /// # Arguments
    /// * `predecessor` - The symbol name (e.g., "A")
    ///
    /// # Returns
    /// A vector of rule source strings, or an empty vector if no rules match.
    pub fn export_rules_for(&self, predecessor: &str) -> Vec<String> {
        let pred_id = match self.interner.resolve_id(predecessor) {
            Some(id) => id,
            None => return vec![],
        };

        let rules = match self.rules.get(&pred_id) {
            Some(r) => r,
            None => return vec![],
        };

        let mut results = Vec::new();
        for rule in rules {
            let config = crate::export::ExportConfig::synthetic(rule);
            if let Ok(source) = crate::export::export_rule_to_string(rule, &self.interner, &config)
            {
                results.push(source);
            }
        }

        results
    }

    /// Exports a specific rule by predecessor and index.
    ///
    /// # Arguments
    /// * `predecessor` - The symbol name (e.g., "A")
    /// * `index` - The index of the rule within the predecessor's rule list
    ///
    /// # Returns
    /// The rule source string, or an error if not found.
    pub fn export_rule_at(&self, predecessor: &str, index: usize) -> Result<String, SystemError> {
        let pred_id = self
            .interner
            .resolve_id(predecessor)
            .ok_or_else(|| SystemError::CompileError(format!("Unknown symbol: {}", predecessor)))?;

        let rules = self
            .rules
            .get(&pred_id)
            .ok_or_else(|| SystemError::CompileError(format!("No rules for: {}", predecessor)))?;

        let rule = rules.get(index).ok_or_else(|| {
            SystemError::CompileError(format!("Rule index {} out of bounds", index))
        })?;

        let config = crate::export::ExportConfig::synthetic(rule);
        crate::export::export_rule_to_string(rule, &self.interner, &config)
            .map_err(SystemError::CompileError)
    }

    /// Exports a rule with custom parameter names.
    ///
    /// This is useful when you want to preserve the original parameter names
    /// or use meaningful names for display purposes.
    ///
    /// # Arguments
    /// * `predecessor` - The symbol name
    /// * `index` - The rule index
    /// * `param_names` - Parameter names for the predecessor
    pub fn export_rule_with_params(
        &self,
        predecessor: &str,
        index: usize,
        param_names: Vec<String>,
    ) -> Result<String, SystemError> {
        let pred_id = self
            .interner
            .resolve_id(predecessor)
            .ok_or_else(|| SystemError::CompileError(format!("Unknown symbol: {}", predecessor)))?;

        let rules = self
            .rules
            .get(&pred_id)
            .ok_or_else(|| SystemError::CompileError(format!("No rules for: {}", predecessor)))?;

        let rule = rules.get(index).ok_or_else(|| {
            SystemError::CompileError(format!("Rule index {} out of bounds", index))
        })?;

        let config = crate::export::ExportConfig {
            predecessor_params: param_names,
            ..Default::default()
        };

        crate::export::export_rule_to_string(rule, &self.interner, &config)
            .map_err(SystemError::CompileError)
    }
}
