use std::collections::{HashMap, HashSet};

use tauri::State;

use super::AppState;
use crate::models::*;

type ProjectModelRow = (
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    u64,
    u64,
    u64,
    f64,
);
type SessionModelRow = (
    String,
    String,
    String,
    String,
    i64,
    String,
    u64,
    u64,
    u64,
    u64,
    u64,
    f64,
);

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
    let cutoff_date = range_days.and_then(cutoff_date_for_range_days);
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
        .map(|(model, turns, inp, out, cr, cw, cost)| ModelCost {
            model,
            turns,
            input_tokens: inp,
            output_tokens: out,
            cache_tokens: cr + cw,
            cost,
        })
        .collect();

    let total_cost: f64 = model_costs.iter().map(|m| m.cost).sum();

    // Project costs: query per (project, provider, session, model) for accurate
    // per-model pricing while deduplicating session counts exactly.
    let project_model_rows = state
        .db
        .usage_project_model_detail(providers, cutoff_ref)
        .map_err(|e| format!("failed to query project model detail: {e}"))?;

    let project_costs = build_project_costs(project_model_rows);

    // Recent sessions: query per (session, model) for accurate per-model pricing,
    // then aggregate by session with the dominant model label.
    let session_model_rows = state
        .db
        .usage_session_model_detail(providers, cutoff_ref, 100)
        .map_err(|e| format!("failed to query session model detail: {e}"))?;

    let recent_sessions = build_recent_sessions(session_model_rows);

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

fn cutoff_date_for_range_days(days: u32) -> Option<String> {
    if days == 0 {
        return None;
    }

    let today = chrono::Local::now().date_naive();
    let cutoff = today - chrono::Duration::days(i64::from(days.saturating_sub(1)));
    Some(cutoff.format("%Y-%m-%d").to_string())
}

fn build_project_costs(project_model_rows: Vec<ProjectModelRow>) -> Vec<ProjectCost> {
    let mut project_map: HashMap<(String, String), ProjectCost> = HashMap::new();
    let mut project_sessions: HashMap<(String, String), HashSet<String>> = HashMap::new();

    for (project_path, project, provider, session_id, _model, turns, inp, out, cr, cw, cost) in
        project_model_rows
    {
        let key = (project_path.clone(), provider.clone());
        project_sessions
            .entry(key.clone())
            .or_default()
            .insert(session_id);

        let entry = project_map.entry(key).or_insert_with(|| ProjectCost {
            project,
            project_path,
            provider,
            sessions: 0,
            turns: 0,
            tokens: 0,
            cost: 0.0,
        });
        entry.turns += turns;
        entry.tokens += inp + out + cr + cw;
        entry.cost += cost;
    }

    let mut project_costs: Vec<ProjectCost> = project_map
        .into_iter()
        .map(|(key, mut cost_row)| {
            cost_row.sessions = project_sessions
                .remove(&key)
                .map(|sessions| sessions.len() as u64)
                .unwrap_or(0);
            cost_row
        })
        .collect();
    project_costs.sort_by(|a, b| {
        b.cost
            .partial_cmp(&a.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    project_costs
}

fn build_recent_sessions(session_model_rows: Vec<SessionModelRow>) -> Vec<SessionCostRow> {
    let mut session_map: HashMap<String, SessionCostRow> = HashMap::new();
    let mut session_order: Vec<String> = Vec::new();
    let mut dominant_model: HashMap<String, (String, u64, f64)> = HashMap::new();

    for (id, project_path, project, provider, updated_at, model, turns, inp, out, cr, cw, cost) in
        session_model_rows
    {
        let tokens = inp + out + cr + cw;
        let entry = session_map.entry(id.clone()).or_insert_with(|| {
            session_order.push(id.clone());
            SessionCostRow {
                id: id.clone(),
                project,
                project_path,
                provider,
                model: String::new(),
                updated_at,
                turns: 0,
                tokens: 0,
                cost: 0.0,
            }
        });
        entry.turns += turns;
        entry.tokens += tokens;
        entry.cost += cost;

        let best = dominant_model
            .entry(id)
            .or_insert_with(|| (model.clone(), tokens, cost));
        if tokens > best.1 || (tokens == best.1 && cost > best.2 && !model.is_empty()) {
            *best = (model, tokens, cost);
        }
    }

    for (id, (model, _, _)) in dominant_model {
        if let Some(entry) = session_map.get_mut(&id) {
            entry.model = model;
        }
    }

    session_order
        .into_iter()
        .filter_map(|id| session_map.remove(&id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{build_project_costs, build_recent_sessions, cutoff_date_for_range_days};

    #[test]
    fn project_costs_count_distinct_sessions_exactly() {
        let rows = vec![
            (
                "/tmp/drama/ccsession".to_string(),
                "drama/ccsession".to_string(),
                "claude".to_string(),
                "session-a".to_string(),
                "opus-4-6".to_string(),
                12,
                100,
                50,
                20,
                10,
                1.0,
            ),
            (
                "/tmp/drama/ccsession".to_string(),
                "drama/ccsession".to_string(),
                "claude".to_string(),
                "session-a".to_string(),
                "sonnet-4-6".to_string(),
                8,
                40,
                10,
                0,
                0,
                0.5,
            ),
            (
                "/tmp/drama/ccsession".to_string(),
                "drama/ccsession".to_string(),
                "claude".to_string(),
                "session-b".to_string(),
                "opus-4-6".to_string(),
                4,
                20,
                10,
                0,
                0,
                0.25,
            ),
        ];

        let project_costs = build_project_costs(rows);
        assert_eq!(project_costs.len(), 1);
        assert_eq!(project_costs[0].sessions, 2);
        assert_eq!(project_costs[0].project_path, "/tmp/drama/ccsession");
        assert_eq!(project_costs[0].turns, 24);
        assert_eq!(project_costs[0].tokens, 260);
    }

    #[test]
    fn recent_sessions_keep_dominant_model_label() {
        let rows = vec![
            (
                "session-a".to_string(),
                "/tmp/drama/ccsession".to_string(),
                "drama/ccsession".to_string(),
                "claude".to_string(),
                1_700_000_000,
                "sonnet-4-6".to_string(),
                6,
                200,
                40,
                0,
                0,
                0.5,
            ),
            (
                "session-a".to_string(),
                "/tmp/drama/ccsession".to_string(),
                "drama/ccsession".to_string(),
                "claude".to_string(),
                1_700_000_000,
                "opus-4-6".to_string(),
                2,
                1_200,
                300,
                0,
                0,
                1.0,
            ),
        ];

        let recent_sessions = build_recent_sessions(rows);
        assert_eq!(recent_sessions.len(), 1);
        assert_eq!(recent_sessions[0].model, "opus-4-6");
        assert_eq!(recent_sessions[0].project_path, "/tmp/drama/ccsession");
        assert_eq!(recent_sessions[0].turns, 8);
        assert_eq!(recent_sessions[0].tokens, 1_740);
    }

    #[test]
    fn project_costs_keep_same_name_different_paths_separate() {
        let rows = vec![
            (
                "/tmp/api-server".to_string(),
                "api-server".to_string(),
                "codex".to_string(),
                "session-a".to_string(),
                "gpt-5.4".to_string(),
                2,
                100,
                40,
                0,
                0,
                0.1,
            ),
            (
                "/work/api-server".to_string(),
                "api-server".to_string(),
                "codex".to_string(),
                "session-b".to_string(),
                "gpt-5.4".to_string(),
                3,
                120,
                60,
                0,
                0,
                0.2,
            ),
        ];

        let project_costs = build_project_costs(rows);
        assert_eq!(project_costs.len(), 2);
    }

    #[test]
    fn cutoff_range_is_inclusive_of_today() {
        let cutoff = cutoff_date_for_range_days(7).expect("cutoff");
        let expected = (chrono::Local::now().date_naive() - chrono::Duration::days(6))
            .format("%Y-%m-%d")
            .to_string();
        assert_eq!(cutoff, expected);
    }
}
