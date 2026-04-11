use crate::models::TokenUsage;

/// Extract token usage from an assistant message's `data.tokens` JSON.
pub fn extract_tokens(msg_json: &serde_json::Value) -> Option<TokenUsage> {
    let tokens = msg_json.get("tokens")?;
    let input = tokens.get("input")?.as_u64()? as u32;
    let output = tokens.get("output")?.as_u64()? as u32;
    let cache = tokens.get("cache");
    let cache_read = cache
        .and_then(|c| c.get("read"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let cache_write = cache
        .and_then(|c| c.get("write"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    Some(TokenUsage {
        input_tokens: input,
        output_tokens: output,
        cache_read_input_tokens: cache_read,
        cache_creation_input_tokens: cache_write,
    })
}

/// Convert epoch milliseconds to RFC3339 timestamp string.
pub fn ms_to_rfc3339(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp(ms / 1000, ((ms % 1000) * 1_000_000) as u32)
        .map(|dt| dt.to_rfc3339())
}
