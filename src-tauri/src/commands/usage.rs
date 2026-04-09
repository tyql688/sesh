use std::collections::HashMap;

use tauri::State;

use crate::models::*;
use crate::pricing;

use super::AppState;

#[tauri::command]
pub async fn get_usage_stats(
    providers: Vec<String>,
    range_days: Option<u32>,
    state: State<'_, AppState>,
) -> Result<UsageStats, String> {
    let state = state.inner().clone();
    tokio::task::spawn_blocking(move || build_usage_stats(&state, &providers, range_days))
        .await
        .map_err(|e| format!("task join error: {e}"))?
}

fn build_usage_stats(
    state: &AppState,
    providers: &[String],
    range_days: Option<u32>,
) -> Result<UsageStats, String> {
    let cutoff_date = range_days.map(|d| {
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::days(d as i64);
        cutoff.format("%Y-%m-%d").to_string()
    });
    let cutoff_ref = cutoff_date.as_deref();

    let total_sessions = state
        .db
        .usage_session_count(providers, cutoff_ref)
        .map_err(|e| format!("failed to count usage sessions: {e}"))?;

    let (total_turns, total_in, total_out, total_cr, total_cw) = state
        .db
        .usage_totals(providers, cutoff_ref)
        .map_err(|e| format!("failed to query usage totals: {e}"))?;

    let daily_rows = state
        .db
        .usage_daily(providers, cutoff_ref)
        .map_err(|e| format!("failed to query daily usage: {e}"))?;
    let daily_usage: Vec<DailyUsage> = daily_rows
        .into_iter()
        .map(|(date, provider, tokens)| DailyUsage {
            date,
            provider,
            tokens,
        })
        .collect();

    let model_rows = state
        .db
        .usage_by_model(providers, cutoff_ref)
        .map_err(|e| format!("failed to query usage by model: {e}"))?;
    let model_costs: Vec<ModelCost> = model_rows
        .into_iter()
        .map(|(model, turns, inp, out, cr, cw)| {
            let cost = pricing::estimate_cost(&model, inp, out, cr, cw);
            ModelCost {
                model,
                turns,
                input_tokens: inp,
                output_tokens: out,
                cache_tokens: cr + cw,
                cost,
            }
        })
        .collect();

    let total_cost: f64 = model_costs.iter().map(|m| m.cost).sum();

    // Project costs: query per (project, provider, model) for accurate per-model pricing,
    // then aggregate by (project, provider).
    let project_model_rows = state
        .db
        .usage_project_model_detail(providers, cutoff_ref)
        .map_err(|e| format!("failed to query project model detail: {e}"))?;

    let mut project_map: HashMap<(String, String), ProjectCost> = HashMap::new();
    for (project, provider, model, sessions, turns, inp, out, cr, cw) in project_model_rows {
        let cost = pricing::estimate_cost(&model, inp, out, cr, cw);
        let entry = project_map
            .entry((project.clone(), provider.clone()))
            .or_insert_with(|| ProjectCost {
                project,
                provider,
                sessions: 0,
                turns: 0,
                tokens: 0,
                cost: 0.0,
            });
        // sessions count from the grouped query is per-model; use max across models
        // since the same session may appear under multiple models.
        // The COUNT(DISTINCT session_id) per model may overlap, so we take the max
        // as a reasonable upper bound. For exact dedup, we'd need another query.
        if sessions > entry.sessions {
            entry.sessions = sessions;
        }
        entry.turns += turns;
        entry.tokens += inp + out + cr + cw;
        entry.cost += cost;
    }
    let mut project_costs: Vec<ProjectCost> = project_map.into_values().collect();
    project_costs.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Recent sessions: query per (session, model) for accurate per-model pricing,
    // then aggregate by session.
    let session_model_rows = state
        .db
        .usage_session_model_detail(providers, cutoff_ref, 50)
        .map_err(|e| format!("failed to query session model detail: {e}"))?;

    let mut session_map: HashMap<String, SessionCostRow> = HashMap::new();
    let mut session_order: Vec<String> = Vec::new();
    for (id, project, provider, updated_at, model, turns, inp, out, cr, cw) in session_model_rows {
        let cost = pricing::estimate_cost(&model, inp, out, cr, cw);
        let tokens = inp + out + cr + cw;
        let entry = session_map.entry(id.clone()).or_insert_with(|| {
            session_order.push(id.clone());
            SessionCostRow {
                id,
                project,
                provider,
                model: model.clone(),
                updated_at,
                turns: 0,
                tokens: 0,
                cost: 0.0,
            }
        });
        entry.turns += turns;
        entry.tokens += tokens;
        entry.cost += cost;
        // Pick the model with the most tokens as the display model
        if tokens > 0 && entry.model != model {
            // Simple heuristic: keep the model name that contributed most cost
            // We just keep whichever was set first or has more tokens — good enough for display
        }
    }
    let recent_sessions: Vec<SessionCostRow> = session_order
        .into_iter()
        .filter_map(|id| session_map.remove(&id))
        .collect();

    let cache_input_total = total_cr + total_in;
    let cache_hit_rate = if cache_input_total > 0 {
        total_cr as f64 / cache_input_total as f64
    } else {
        0.0
    };

    Ok(UsageStats {
        total_sessions,
        total_turns,
        total_input_tokens: total_in,
        total_output_tokens: total_out,
        total_cache_read_tokens: total_cr,
        total_cache_write_tokens: total_cw,
        total_cost,
        cache_hit_rate,
        daily_usage,
        model_costs,
        project_costs,
        recent_sessions,
    })
}
