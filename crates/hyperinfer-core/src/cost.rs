//! Weighted token-cost calculation for LLM usage.
//!
//! Supports per-model cost rules with prefix matching, and distinguishes
//! between input, cached input, output, and reasoning tokens.

use crate::Usage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Weighted token usage breakdown.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WeightedUsage {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub reasoning_tokens: u64,
}

impl From<&Usage> for WeightedUsage {
    fn from(usage: &Usage) -> Self {
        Self {
            input_tokens: usage.input_tokens as u64,
            cached_input_tokens: usage.cache_read_tokens.unwrap_or(0) as u64,
            output_tokens: usage.output_tokens as u64,
            reasoning_tokens: 0,
        }
    }
}

/// Per-model cost weights in cents per 1M tokens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenCostWeights {
    pub input_cents_per_1m: f64,
    pub cached_input_cents_per_1m: f64,
    pub output_cents_per_1m: f64,
    pub reasoning_cents_per_1m: f64,
}

/// Cost calculator with per-model rules and prefix matching.
#[derive(Debug, Clone)]
pub struct CostCalculator {
    rules: HashMap<String, TokenCostWeights>,
    default_weights: TokenCostWeights,
}

impl CostCalculator {
    /// Create a calculator with realistic default rates (in cents per 1M tokens) for popular models.
    pub fn with_defaults() -> Self {
        let mut calc = Self {
            rules: HashMap::new(),
            default_weights: TokenCostWeights {
                input_cents_per_1m: 250.0,
                cached_input_cents_per_1m: 125.0,
                output_cents_per_1m: 1000.0,
                reasoning_cents_per_1m: 1000.0,
            },
        };

        // gpt-4o: $2.50/M input (250c), $10/M output (1000c)
        calc.set_rule(
            "gpt-4o",
            TokenCostWeights {
                input_cents_per_1m: 250.0,
                cached_input_cents_per_1m: 125.0,
                output_cents_per_1m: 1000.0,
                reasoning_cents_per_1m: 1000.0,
            },
        );

        // gpt-4o-mini: $0.15/M input (15c), $0.60/M output (60c)
        calc.set_rule(
            "gpt-4o-mini",
            TokenCostWeights {
                input_cents_per_1m: 15.0,
                cached_input_cents_per_1m: 7.5,
                output_cents_per_1m: 60.0,
                reasoning_cents_per_1m: 60.0,
            },
        );

        // claude-sonnet-4: $3/M input (300c), $15/M output (1500c)
        calc.set_rule(
            "claude-sonnet-4",
            TokenCostWeights {
                input_cents_per_1m: 300.0,
                cached_input_cents_per_1m: 150.0,
                output_cents_per_1m: 1500.0,
                reasoning_cents_per_1m: 1500.0,
            },
        );

        // gemini-2.5-flash: $0.075/M input (7.5c), $0.30/M output (30c)
        calc.set_rule(
            "gemini-2.5-flash",
            TokenCostWeights {
                input_cents_per_1m: 7.5,
                cached_input_cents_per_1m: 3.75,
                output_cents_per_1m: 30.0,
                reasoning_cents_per_1m: 30.0,
            },
        );

        calc
    }

    /// Set a cost rule for a model prefix.
    pub fn set_rule(&mut self, model_prefix: impl Into<String>, weights: TokenCostWeights) {
        self.rules.insert(model_prefix.into(), weights);
    }

    /// Get the cost weights for a model (exact match, then longest prefix, then default).
    pub fn weights_for(&self, model: &str) -> &TokenCostWeights {
        if let Some(weights) = self.rules.get(model) {
            return weights;
        }

        let mut best_prefix = "";
        let mut best_weights = &self.default_weights;
        for (prefix, weights) in &self.rules {
            if model.starts_with(prefix) && prefix.len() > best_prefix.len() {
                best_prefix = prefix;
                best_weights = weights;
            }
        }
        best_weights
    }

    /// Calculate cost in cents for the given usage.
    /// Non-cached input = input_tokens - cached_input_tokens (no double-charging).
    pub fn calculate_cost_cents(&self, model: &str, usage: &WeightedUsage) -> f64 {
        let weights = self.weights_for(model);
        let non_cached_input = usage.input_tokens.saturating_sub(usage.cached_input_tokens);

        let input_cost = (non_cached_input as f64) * weights.input_cents_per_1m / 1_000_000.0;
        let cached_cost =
            (usage.cached_input_tokens as f64) * weights.cached_input_cents_per_1m / 1_000_000.0;
        let output_cost = (usage.output_tokens as f64) * weights.output_cents_per_1m / 1_000_000.0;
        let reasoning_cost =
            (usage.reasoning_tokens as f64) * weights.reasoning_cents_per_1m / 1_000_000.0;

        input_cost + cached_cost + output_cost + reasoning_cost
    }

    /// Calculate cost in cents, rounded up to nearest cent (minimum 0).
    pub fn calculate_cost_cents_ceil(&self, model: &str, usage: &WeightedUsage) -> i64 {
        let cost = self.calculate_cost_cents(model, usage);
        cost.max(0.0).ceil() as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_weighted_cost_calculation() {
        let calc = CostCalculator::with_defaults();
        let usage = WeightedUsage {
            input_tokens: 1_000_000,
            cached_input_tokens: 200_000,
            output_tokens: 100_000,
            reasoning_tokens: 50_000,
        };

        // gpt-4o: 250c/M input, 125c/M cached, 1000c/M output, 1000c/M reasoning
        // non-cached input: 800k * 250/1M = 200.0
        // cached input: 200k * 125/1M = 25.0
        // output: 100k * 1000/1M = 100.0
        // reasoning: 50k * 1000/1M = 50.0
        // total: 375.0 cents ($3.75)
        let cost = calc.calculate_cost_cents("gpt-4o", &usage);
        assert!((cost - 375.0).abs() < 0.001);
    }

    #[test]
    fn test_custom_rule_override() {
        let mut calc = CostCalculator::with_defaults();
        calc.set_rule(
            "custom-model",
            TokenCostWeights {
                input_cents_per_1m: 100.0,
                cached_input_cents_per_1m: 50.0,
                output_cents_per_1m: 200.0,
                reasoning_cents_per_1m: 200.0,
            },
        );

        let usage = WeightedUsage {
            input_tokens: 1_000_000,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
        };

        let cost = calc.calculate_cost_cents("custom-model", &usage);
        assert!((cost - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_prefix_matching() {
        let mut calc = CostCalculator::with_defaults();
        calc.set_rule(
            "gpt-4",
            TokenCostWeights {
                input_cents_per_1m: 50.0,
                cached_input_cents_per_1m: 25.0,
                output_cents_per_1m: 100.0,
                reasoning_cents_per_1m: 100.0,
            },
        );

        // Exact match should win over prefix
        let usage = WeightedUsage {
            input_tokens: 1_000_000,
            cached_input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: 0,
        };

        // gpt-4o has exact match at 250c/M ($2.50/M)
        let cost_o = calc.calculate_cost_cents("gpt-4o", &usage);
        assert!((cost_o - 250.0).abs() < 0.001);

        // gpt-4-turbo matches prefix "gpt-4" at $50/M
        let cost_turbo = calc.calculate_cost_cents("gpt-4-turbo", &usage);
        assert!((cost_turbo - 50.0).abs() < 0.001);
    }
}
