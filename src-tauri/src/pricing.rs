/// Per-token costs in USD.
///
/// Priority order:
/// 1. Exact or near-exact mappings for models currently observed in the index.
/// 2. Family fallbacks for older or less specific model names.
///
/// Major rates here were refreshed from official pricing pages for OpenAI,
/// Anthropic, Google Gemini, and Moonshot/Kimi in April 2026. Providers/models
/// without stable public USD pricing remain best-effort fallbacks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelPricing {
    pub input: f64,
    pub output: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub input_above_threshold: Option<f64>,
    pub output_above_threshold: Option<f64>,
    pub cache_read_above_threshold: Option<f64>,
    pub cache_write_above_threshold: Option<f64>,
    pub threshold_tokens: Option<u64>,
}

const fn usd_per_token(
    input_per_million: f64,
    output_per_million: f64,
    cache_read_per_million: f64,
    cache_write_per_million: f64,
) -> ModelPricing {
    ModelPricing {
        input: input_per_million / 1_000_000.0,
        output: output_per_million / 1_000_000.0,
        cache_read: cache_read_per_million / 1_000_000.0,
        cache_write: cache_write_per_million / 1_000_000.0,
        input_above_threshold: None,
        output_above_threshold: None,
        cache_read_above_threshold: None,
        cache_write_above_threshold: None,
        threshold_tokens: None,
    }
}

const fn usd_per_token_tiered(
    base_per_million: [f64; 4],
    above_threshold_per_million: [f64; 4],
    threshold_tokens: u64,
) -> ModelPricing {
    ModelPricing {
        input: base_per_million[0] / 1_000_000.0,
        output: base_per_million[1] / 1_000_000.0,
        cache_read: base_per_million[2] / 1_000_000.0,
        cache_write: base_per_million[3] / 1_000_000.0,
        input_above_threshold: Some(above_threshold_per_million[0] / 1_000_000.0),
        output_above_threshold: Some(above_threshold_per_million[1] / 1_000_000.0),
        cache_read_above_threshold: Some(above_threshold_per_million[2] / 1_000_000.0),
        cache_write_above_threshold: Some(above_threshold_per_million[3] / 1_000_000.0),
        threshold_tokens: Some(threshold_tokens),
    }
}

fn contains_any(model: &str, aliases: &[&str]) -> bool {
    aliases.iter().any(|alias| model.contains(alias))
}

/// Look up pricing by model-name matching.
pub fn lookup_pricing(model: &str) -> Option<ModelPricing> {
    let m = model.to_lowercase();

    // Anthropic Claude
    if contains_any(&m, &["claude-opus-4-6", "opus-4-6", "opus-4-5"]) {
        return Some(usd_per_token(5.0, 25.0, 0.5, 6.25));
    }
    if contains_any(&m, &["claude-opus-4-1", "opus-4-1", "opus-4-0", "opus-3"]) {
        return Some(usd_per_token(15.0, 75.0, 1.5, 18.75));
    }
    if contains_any(&m, &["claude-sonnet-4-5", "sonnet-4-5"]) {
        return Some(usd_per_token_tiered(
            [3.0, 15.0, 0.3, 3.75],
            [6.0, 22.5, 0.6, 7.5],
            200_000,
        ));
    }
    if contains_any(&m, &["claude-sonnet-4-6", "sonnet-4-6", "sonnet"]) {
        return Some(usd_per_token(3.0, 15.0, 0.3, 3.75));
    }
    if contains_any(&m, &["claude-haiku-4-5", "haiku-4-5", "haiku"]) {
        return Some(usd_per_token(1.0, 5.0, 0.1, 1.25));
    }

    // OpenAI / Codex
    if contains_any(&m, &["gpt-5.4-mini", "gpt-5-mini", "codex-mini"]) {
        return Some(usd_per_token(0.75, 4.5, 0.075, 0.0));
    }
    if contains_any(&m, &["gpt-5.4", "gpt-5-codex"]) {
        return Some(usd_per_token(2.5, 15.0, 0.25, 0.0));
    }
    if contains_any(&m, &["gpt-5.3", "gpt-5.2", "gpt-5.1-codex"]) {
        return Some(usd_per_token(1.75, 14.0, 0.175, 0.0));
    }
    if contains_any(&m, &["gpt-5"]) {
        return Some(usd_per_token(1.25, 10.0, 0.125, 0.0));
    }

    // Google Gemini
    if contains_any(&m, &["gemini-2.5-pro", "gemini-2-5-pro"]) {
        return Some(usd_per_token(1.25, 10.0, 0.125, 0.0));
    }
    if contains_any(&m, &["gemini-2.5-flash", "gemini-2-5-flash"]) {
        return Some(usd_per_token(0.3, 2.5, 0.03, 0.0));
    }
    if contains_any(
        &m,
        &["gemini-3-flash-preview", "gemini-3-flash", "gemini-3"],
    ) {
        return Some(usd_per_token(0.5, 3.0, 0.05, 0.0));
    }

    // Moonshot / Kimi
    if contains_any(&m, &["kimi-k2.5", "k2.5"]) {
        return Some(usd_per_token(0.6, 3.0, 0.1, 0.6));
    }
    if contains_any(&m, &["kimi-k2", "k2-0905", "kimi k2"]) {
        return Some(usd_per_token(0.6, 2.5, 0.15, 0.6));
    }

    // Best-effort fallbacks for providers without exact public USD mappings here.
    if contains_any(&m, &["glm-5.1", "glm-5"]) {
        return Some(usd_per_token(0.55, 2.2, 0.14, 0.0));
    }
    if contains_any(
        &m,
        &["minimax-m2.7-highspeed", "minimax-m2.7", "m2.7-highspeed"],
    ) {
        return Some(usd_per_token(1.1, 8.0, 0.2, 0.0));
    }
    if contains_any(&m, &["qwen", "coder-model"]) {
        return Some(usd_per_token(0.4, 1.2, 0.1, 0.0));
    }

    None
}

/// Calculate estimated cost for given token counts and model.
pub fn estimate_cost(
    model: &str,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
) -> f64 {
    let Some(p) = lookup_pricing(model) else {
        return 0.0;
    };
    component_cost(input, p.input, p.input_above_threshold, p.threshold_tokens)
        + component_cost(
            output,
            p.output,
            p.output_above_threshold,
            p.threshold_tokens,
        )
        + component_cost(
            cache_read,
            p.cache_read,
            p.cache_read_above_threshold,
            p.threshold_tokens,
        )
        + component_cost(
            cache_write,
            p.cache_write,
            p.cache_write_above_threshold,
            p.threshold_tokens,
        )
}

fn component_cost(
    tokens: u64,
    base_price: f64,
    above_threshold_price: Option<f64>,
    threshold_tokens: Option<u64>,
) -> f64 {
    if tokens == 0 {
        return 0.0;
    }
    match (above_threshold_price, threshold_tokens) {
        (Some(above), Some(threshold)) if tokens > threshold => {
            let below = threshold as f64 * base_price;
            let above_tokens = (tokens - threshold) as f64 * above;
            below + above_tokens
        }
        _ => tokens as f64 * base_price,
    }
}

#[cfg(test)]
mod tests {
    use super::{estimate_cost, lookup_pricing};

    fn assert_close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    #[test]
    fn lookup_pricing_matches_gpt_54_exactly() {
        let pricing = lookup_pricing("gpt-5.4").expect("pricing");
        assert_close(pricing.input, 2.5e-6);
        assert_close(pricing.output, 15.0e-6);
        assert_close(pricing.cache_read, 0.25e-6);
        assert_eq!(pricing.threshold_tokens, None);
    }

    #[test]
    fn lookup_pricing_matches_gpt_54_mini_exactly() {
        let pricing = lookup_pricing("gpt-5.4-mini").expect("pricing");
        assert_close(pricing.input, 0.75e-6);
        assert_close(pricing.output, 4.5e-6);
        assert_close(pricing.cache_read, 0.075e-6);
    }

    #[test]
    fn lookup_pricing_matches_gemini_3_flash_preview() {
        let pricing = lookup_pricing("gemini-3-flash-preview").expect("pricing");
        assert_close(pricing.input, 0.5e-6);
        assert_close(pricing.output, 3.0e-6);
        assert_close(pricing.cache_read, 0.05e-6);
    }

    #[test]
    fn estimate_cost_handles_tiered_pricing() {
        let cost = estimate_cost("claude-sonnet-4-5", 300_000, 250_000, 250_000, 300_000);
        let expected = (200_000.0 * 3e-6)
            + (100_000.0 * 6e-6)
            + (200_000.0 * 15e-6)
            + (50_000.0 * 22.5e-6)
            + (200_000.0 * 0.3e-6)
            + (50_000.0 * 0.6e-6)
            + (200_000.0 * 3.75e-6)
            + (100_000.0 * 7.5e-6);
        assert!((cost - expected).abs() < 1e-12);
    }

    #[test]
    fn estimate_cost_uses_cache_components() {
        let cost = estimate_cost("kimi-k2.5", 100, 50, 20, 10);
        let expected = (100.0 * 0.6e-6) + (50.0 * 3.0e-6) + (20.0 * 0.1e-6) + (10.0 * 0.6e-6);
        assert!((cost - expected).abs() < 1e-12);
    }
}
