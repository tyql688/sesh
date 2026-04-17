use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::models::{Message, MessageRole, Provider, SessionMeta, TokenUsage};
use crate::provider::ParsedSession;
use crate::provider_utils::{
    is_system_content, parse_rfc3339_timestamp, project_name_from_path, session_title,
    truncate_to_bytes, FTS_CONTENT_LIMIT, NO_PROJECT,
};
use crate::tool_metadata::{
    build_tool_metadata, enrich_tool_metadata, ToolCallFacts, ToolResultFacts,
};

use super::tools::*;
use super::CodexProvider;

#[derive(Clone, Debug)]
pub struct CodexUsageEvent {
    pub timestamp: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
}

#[derive(Deserialize)]
struct CodexLine {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    line_type: String,
    payload: Option<Value>,
}

struct PendingCodexUserMessage {
    content: String,
    timestamp: Option<String>,
    image_segments: Vec<String>,
}

fn parse_json_str(value: Option<&str>) -> Option<Value> {
    serde_json::from_str(value?).ok()
}

fn codex_tool_input_value(
    raw_name: &str,
    raw_input: Option<&str>,
    tool_input: Option<&str>,
) -> Option<Value> {
    if raw_name == "apply_patch" {
        return tool_input.map(|patch| json!({ "patch": patch }));
    }

    parse_json_str(tool_input).or_else(|| parse_json_str(raw_input))
}

fn codex_tool_result_value(raw_output: &str, output: &str) -> Option<Value> {
    if let Ok(value) = serde_json::from_str::<Value>(raw_output.trim()) {
        if let Some(obj) = value.as_object() {
            if let Some(metadata) = obj.get("metadata").and_then(|v| v.as_object()) {
                let mut result = serde_json::Map::new();
                result.insert("stdout".to_string(), json!(output));
                if let Some(exit_code) = metadata.get("exit_code") {
                    result.insert("exitCode".to_string(), exit_code.clone());
                }
                if let Some(duration) = metadata.get("duration_seconds") {
                    result.insert("durationSeconds".to_string(), duration.clone());
                }
                return Some(Value::Object(result));
            }
        }
        return Some(value);
    }

    if output.trim().is_empty() {
        None
    } else {
        Some(json!({ "stdout": output }))
    }
}

fn codex_duration_seconds(value: Option<&Value>) -> Option<f64> {
    let duration = value?.as_object()?;
    let secs = duration.get("secs").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let nanos = duration
        .get("nanos")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    Some(secs + nanos / 1_000_000_000.0)
}

fn codex_exec_command_event_result(payload: &Value, fallback_output: &str) -> Value {
    let mut result = Map::new();
    if let Some(command) = payload.get("command") {
        result.insert("command".to_string(), command.clone());
    }
    if let Some(cwd) = payload.get("cwd") {
        result.insert("cwd".to_string(), cwd.clone());
    }
    if let Some(parsed_cmd) = payload.get("parsed_cmd") {
        result.insert("parsedCmd".to_string(), parsed_cmd.clone());
    }
    if let Some(source) = payload.get("source") {
        result.insert("source".to_string(), source.clone());
    }
    if let Some(status) = payload.get("status") {
        result.insert("status".to_string(), status.clone());
    }
    if let Some(process_id) = payload.get("process_id") {
        result.insert("processId".to_string(), process_id.clone());
    }
    if let Some(interaction_input) = payload.get("interaction_input") {
        result.insert("interactionInput".to_string(), interaction_input.clone());
    }
    if let Some(exit_code) = payload.get("exit_code") {
        result.insert("exitCode".to_string(), exit_code.clone());
    }
    if let Some(duration) = codex_duration_seconds(payload.get("duration")) {
        result.insert("durationSeconds".to_string(), json!(duration));
    }

    let stdout = payload
        .get("stdout")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback_output);
    if !stdout.is_empty() {
        result.insert("stdout".to_string(), json!(stdout));
    }
    if let Some(stderr) = payload.get("stderr").and_then(|v| v.as_str()) {
        if !stderr.is_empty() {
            result.insert("stderr".to_string(), json!(stderr));
        }
    }
    if let Some(aggregated_output) = payload.get("aggregated_output").and_then(|v| v.as_str()) {
        if !aggregated_output.is_empty() {
            result.insert("aggregatedOutput".to_string(), json!(aggregated_output));
        }
    }
    if let Some(formatted_output) = payload.get("formatted_output").and_then(|v| v.as_str()) {
        if !formatted_output.is_empty() {
            result.insert("formattedOutput".to_string(), json!(formatted_output));
        }
    }

    Value::Object(result)
}

fn codex_patch_event_patch(path: &str, change: &Value) -> Option<Value> {
    let change_type = change.get("type").and_then(|v| v.as_str())?;
    let mut patch = Map::new();
    patch.insert("files".to_string(), json!([path]));
    patch.insert("changeType".to_string(), json!(change_type));
    if let Some(move_path) = change.get("move_path").and_then(|v| v.as_str()) {
        patch.insert("movePath".to_string(), json!(move_path));
    }
    if let Some(unified_diff) = change.get("unified_diff").and_then(|v| v.as_str()) {
        patch.insert("diff".to_string(), json!(unified_diff));
    }
    Some(Value::Object(patch))
}

fn codex_patch_event_result(payload: &Value) -> Value {
    let mut result = Map::new();
    if let Some(stdout) = payload.get("stdout") {
        result.insert("stdout".to_string(), stdout.clone());
    }
    if let Some(stderr) = payload.get("stderr") {
        result.insert("stderr".to_string(), stderr.clone());
    }
    if let Some(success) = payload.get("success") {
        result.insert("success".to_string(), success.clone());
    }
    if let Some(status) = payload.get("status") {
        result.insert("status".to_string(), status.clone());
    }

    let mut combined = Vec::new();
    let mut patches = Vec::new();
    if let Some(changes) = payload.get("changes") {
        result.insert("changes".to_string(), changes.clone());
        if let Some(change_map) = changes.as_object() {
            for (path, change) in change_map {
                if let Some(patch) = codex_patch_event_patch(path, change) {
                    patches.push(patch);
                }
                let header = match change.get("type").and_then(|v| v.as_str()) {
                    Some("add") => format!("*** Add File: {path}"),
                    Some("delete") => format!("*** Delete File: {path}"),
                    _ => format!("*** Update File: {path}"),
                };
                combined.push(header);
                if let Some(move_path) = change.get("move_path").and_then(|v| v.as_str()) {
                    combined.push(format!("*** Move to: {move_path}"));
                }
                if let Some(unified_diff) = change.get("unified_diff").and_then(|v| v.as_str()) {
                    combined.push(unified_diff.to_string());
                }
            }
        }
    }

    if !patches.is_empty() {
        result.insert("patches".to_string(), Value::Array(patches));
    }
    if !combined.is_empty() {
        result.insert("diff".to_string(), json!(combined.join("\n")));
    }

    Value::Object(result)
}

fn codex_mcp_tool_call_event_result(payload: &Value) -> Value {
    let mut result = Map::new();
    if let Some(invocation) = payload.get("invocation") {
        result.insert("invocation".to_string(), invocation.clone());
    }
    if let Some(raw_result) = payload.get("result") {
        result.insert("result".to_string(), raw_result.clone());
        let success = raw_result.get("Err").is_none()
            && !raw_result
                .get("Ok")
                .and_then(|ok| ok.get("is_error"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
        result.insert("success".to_string(), json!(success));
    }
    if let Some(duration) = codex_duration_seconds(payload.get("duration")) {
        result.insert("durationSeconds".to_string(), json!(duration));
    }
    Value::Object(result)
}

fn merge_tool_result(existing: Option<&Value>, update: &Value) -> Value {
    match (existing.and_then(Value::as_object), update.as_object()) {
        (Some(existing), Some(update)) => {
            let mut merged = existing.clone();
            for (key, value) in update {
                merged.insert(key.clone(), value.clone());
            }
            Value::Object(merged)
        }
        _ => update.clone(),
    }
}

fn enrich_existing_tool_message(
    message: &mut Message,
    raw_result: Value,
    is_error: Option<bool>,
    status: Option<&str>,
) {
    let Some(metadata) = message.tool_metadata.as_mut() else {
        return;
    };
    let merged = merge_tool_result(metadata.structured.as_ref(), &raw_result);
    enrich_tool_metadata(
        metadata,
        ToolResultFacts {
            raw_result: Some(&merged),
            is_error,
            status,
            artifact_path: None,
        },
    );
}

impl CodexProvider {
    pub fn parse_session_file(&self, path: &PathBuf) -> Option<ParsedSession> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) => {
                log::warn!(
                    "failed to open Codex session '{}': {}",
                    path.display(),
                    error
                );
                return None;
            }
        };
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) => {
                log::warn!(
                    "failed to read Codex session metadata '{}': {}",
                    path.display(),
                    error
                );
                return None;
            }
        };
        let file_size = metadata.len();

        let reader = BufReader::new(file);
        let mut messages = Vec::new();
        let mut first_user_message: Option<String> = None;
        let mut first_timestamp: Option<String> = None;
        let mut last_timestamp: Option<String> = None;
        let mut content_parts: Vec<String> = Vec::new();
        let mut session_id: Option<String> = None;
        let mut cwd: Option<String> = None;
        // Map call_id -> message index for merging function_call_output into function_call
        let mut call_id_map: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut model: Option<String> = None;
        let mut model_provider: Option<String> = None;
        let mut current_model: Option<String> = None;
        let mut cc_version: Option<String> = None;
        let mut git_branch: Option<String> = None;
        let mut is_sidechain = false;
        let mut parent_id: Option<String> = None;
        let mut agent_nickname: Option<String> = None;
        let mut pending_user_message: Option<PendingCodexUserMessage> = None;
        // When parsing subagent files, skip the forked parent context.
        // Subagent JSONL: [sub_meta, parent_meta, ...parent_context..., spawn_marker, sub_turn]
        // The marker "You are the newly spawned agent" signals end of parent context.
        let mut skipping_fork_context = false;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(error) => {
                    log::warn!(
                        "failed to read Codex session line from '{}': {}",
                        path.display(),
                        error
                    );
                    continue;
                }
            };
            if line.trim().is_empty() {
                continue;
            }

            let entry: CodexLine = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(error) => {
                    log::warn!(
                        "skipping malformed Codex JSONL in '{}': {}",
                        path.display(),
                        error
                    );
                    continue;
                }
            };

            if let Some(ref ts) = entry.timestamp {
                if first_timestamp.is_none() {
                    first_timestamp = Some(ts.clone());
                }
                last_timestamp = Some(ts.clone());
            }

            let payload = match entry.payload {
                Some(ref p) => p,
                None => continue,
            };

            // Skip forked parent context in subagent files.
            // End marker: function_call_output containing "newly spawned agent".
            if skipping_fork_context {
                if entry.line_type == "response_item" {
                    let item_type = payload.get("type").and_then(|v| v.as_str());
                    if item_type == Some("function_call_output") {
                        let output = payload.get("output").and_then(|v| v.as_str()).unwrap_or("");
                        if output.contains("newly spawned agent") {
                            skipping_fork_context = false;
                        }
                    }
                }
                continue;
            }

            match entry.line_type.as_str() {
                "session_meta" => {
                    // Only process the first session_meta; subagent JSONL files
                    // contain a second session_meta for the parent context which
                    // would overwrite the subagent's own id/cwd/source fields.
                    if session_id.is_some() {
                        // 2nd session_meta = start of forked parent context
                        if is_sidechain {
                            skipping_fork_context = true;
                        }
                        continue;
                    }
                    if let Some(id) = payload.get("id").and_then(|v| v.as_str()) {
                        session_id = Some(id.to_string());
                    }
                    if let Some(c) = payload.get("cwd").and_then(|v| v.as_str()) {
                        cwd = Some(c.to_string());
                    }
                    if let Some(v) = payload.get("cli_version").and_then(|v| v.as_str()) {
                        if !v.is_empty() {
                            cc_version = Some(v.to_string());
                        }
                    }
                    if let Some(m) = payload.get("model_provider").and_then(|v| v.as_str()) {
                        if !m.is_empty() {
                            model_provider = Some(m.to_string());
                        }
                    }
                    if let Some(b) = payload
                        .get("git")
                        .and_then(|g| g.get("branch"))
                        .and_then(|v| v.as_str())
                    {
                        if !b.is_empty() && b != "HEAD" {
                            git_branch = Some(b.to_string());
                        }
                    }
                    // Detect subagent sessions: source.subagent.thread_spawn
                    if let Some(spawn) = payload
                        .get("source")
                        .and_then(|s| s.get("subagent"))
                        .and_then(|a| a.get("thread_spawn"))
                    {
                        is_sidechain = true;
                        parent_id = spawn
                            .get("parent_thread_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        agent_nickname = payload
                            .get("agent_nickname")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                    }
                }
                "response_item" => {
                    // Skip developer role and reasoning type
                    let role_str = payload.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    let item_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

                    if role_str == "developer" || item_type == "reasoning" {
                        continue;
                    }

                    if !(item_type == "message" && role_str == "user") {
                        flush_pending_user_message(
                            &mut pending_user_message,
                            &mut messages,
                            &mut content_parts,
                            &mut first_user_message,
                        );
                    }

                    match item_type {
                        "message" => {
                            let text = extract_codex_content(payload);
                            let normalized_text = strip_inline_image_sources(&text);
                            let role = match role_str {
                                "user" => MessageRole::User,
                                "assistant" => MessageRole::Assistant,
                                _ => continue,
                            };

                            // Skip empty messages and system/environment XML content
                            if text.is_empty() {
                                continue;
                            }
                            let trimmed = normalized_text.trim_start();
                            if is_system_content(trimmed) {
                                continue;
                            }

                            if role == MessageRole::User {
                                let image_segments = extract_image_source_segments(&text);
                                flush_pending_user_message(
                                    &mut pending_user_message,
                                    &mut messages,
                                    &mut content_parts,
                                    &mut first_user_message,
                                );
                                pending_user_message = Some(PendingCodexUserMessage {
                                    content: text,
                                    timestamp: entry.timestamp.clone(),
                                    image_segments,
                                });
                                continue;
                            }

                            let msg_model = if role == MessageRole::Assistant {
                                current_model.clone()
                            } else {
                                None
                            };
                            if !normalized_text.is_empty() {
                                content_parts.push(normalized_text);
                            }
                            messages.push(Message {
                                role,
                                content: text,
                                timestamp: entry.timestamp.clone(),
                                tool_name: None,
                                tool_input: None,
                                token_usage: None,
                                model: msg_model,
                                usage_hash: None,
                                tool_metadata: None,
                            });
                        }
                        "function_call" => {
                            let raw_name = payload
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let arguments_str = payload.get("arguments").and_then(|v| v.as_str());

                            // For exec_command, remap arguments to match Bash tool format
                            let tool_input = match raw_name {
                                "exec_command" | "shell_command" => {
                                    // Remap {"cmd": "..."} to {"command": "..."}; keep already-normalized command args.
                                    arguments_str.and_then(|s| {
                                        let v: Value = match serde_json::from_str(s) {
                                            Ok(value) => value,
                                            Err(error) => {
                                                log::warn!(
                                                    "failed to parse Codex tool arguments in '{}': {}",
                                                    path.display(),
                                                    error
                                                );
                                                return None;
                                            }
                                        };
                                        let cmd = v
                                            .get("cmd")
                                            .or_else(|| v.get("command"))
                                            .and_then(|c| c.as_str())?;
                                        Some(json!({"command": cmd}).to_string())
                                    })
                                }
                                "view_image" => {
                                    // Emit as image message instead of tool
                                    if let Some(path) = arguments_str.and_then(|s| {
                                        match serde_json::from_str::<Value>(s) {
                                            Ok(value) => value
                                                .get("path")
                                                .and_then(|p| p.as_str())
                                                .map(|s| s.to_string()),
                                            Err(error) => {
                                                log::warn!(
                                                    "failed to parse Codex view_image arguments in '{}': {}",
                                                    path.display(),
                                                    error
                                                );
                                                None
                                            }
                                        }
                                    })
                                    {
                                        messages.push(Message {
                                            role: MessageRole::Assistant,
                                            content: format!("[Image: source: {path}]"),
                                            timestamp: entry.timestamp.clone(),
                                            tool_name: None,
                                            tool_input: None,
                                            token_usage: None,
                                            model: None,
                                            usage_hash: None,
                                            tool_metadata: None,
                                        });
                                        continue;
                                    }
                                    None
                                }
                                "write_stdin" => {
                                    // Skip empty stdin writes (just polling)
                                    let is_empty = arguments_str
                                        .and_then(|s| serde_json::from_str::<Value>(s).ok())
                                        .and_then(|v| {
                                            v.get("chars")
                                                .and_then(|c| c.as_str())
                                                .map(|s| s.is_empty())
                                        })
                                        .unwrap_or(true);
                                    if is_empty {
                                        continue;
                                    }
                                    arguments_str.map(|s| s.to_string())
                                }
                                _ => arguments_str.map(|s| s.to_string()),
                            };
                            let input_value = codex_tool_input_value(
                                raw_name,
                                arguments_str,
                                tool_input.as_deref(),
                            );
                            let metadata = build_tool_metadata(ToolCallFacts {
                                provider: Provider::Codex,
                                raw_name,
                                input: input_value.as_ref(),
                                call_id: payload.get("call_id").and_then(|v| v.as_str()),
                                assistant_id: None,
                            });
                            let display_name = metadata.canonical_name.clone();

                            let idx = messages.len();
                            if let Some(cid) = payload.get("call_id").and_then(|v| v.as_str()) {
                                call_id_map.insert(cid.to_string(), idx);
                            }
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: String::new(),
                                timestamp: entry.timestamp.clone(),
                                tool_name: Some(display_name.to_string()),
                                tool_input,
                                tool_metadata: Some(metadata),
                                token_usage: None,
                                model: None,
                                usage_hash: None,
                            });
                        }
                        "function_call_output" => {
                            let raw_output = match payload.get("output") {
                                Some(Value::String(s)) => s.clone(),
                                Some(other) => serde_json::to_string(other).unwrap_or_default(),
                                None => String::new(),
                            };
                            let output = extract_tool_output(&raw_output);

                            if !output.is_empty() {
                                content_parts.push(output.clone());
                            }

                            // Merge output into the matching function_call message
                            let call_id = payload.get("call_id").and_then(|v| v.as_str());
                            if let Some(idx) = call_id.and_then(|cid| call_id_map.get(cid)).copied()
                            {
                                if idx < messages.len() {
                                    let result_value =
                                        codex_tool_result_value(&raw_output, &output);
                                    messages[idx].content = output;
                                    let is_error = result_value.as_ref().and_then(|value| {
                                        value
                                            .get("exitCode")
                                            .and_then(|code| code.as_i64())
                                            .map(|code| code != 0)
                                    });
                                    if let Some(result_value) = result_value {
                                        enrich_existing_tool_message(
                                            &mut messages[idx],
                                            result_value,
                                            is_error,
                                            None,
                                        );
                                    }
                                    continue;
                                }
                            }
                            // Fallback: standalone output message
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: output,
                                timestamp: entry.timestamp.clone(),
                                tool_name: None,
                                tool_input: None,
                                token_usage: None,
                                model: None,
                                usage_hash: None,
                                tool_metadata: None,
                            });
                        }
                        "web_search_call" => {
                            let action = payload.get("action");
                            let call_id = payload.get("id").and_then(|v| v.as_str());
                            let query = action
                                .and_then(|a| a.get("query"))
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let mut metadata = build_tool_metadata(ToolCallFacts {
                                provider: Provider::Codex,
                                raw_name: "web_search_call",
                                input: action,
                                call_id,
                                assistant_id: None,
                            });
                            enrich_tool_metadata(
                                &mut metadata,
                                ToolResultFacts {
                                    raw_result: Some(payload),
                                    is_error: payload
                                        .get("status")
                                        .and_then(|v| v.as_str())
                                        .map(|status| matches!(status, "failed" | "error")),
                                    status: payload.get("status").and_then(|v| v.as_str()),
                                    artifact_path: None,
                                },
                            );
                            if !query.is_empty() {
                                content_parts.push(query.to_string());
                            }
                            let idx = messages.len();
                            if let Some(call_id) = call_id {
                                call_id_map.insert(call_id.to_string(), idx);
                            }
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: query.to_string(),
                                timestamp: entry.timestamp.clone(),
                                tool_name: Some(metadata.canonical_name.clone()),
                                tool_input: action.map(|value| value.to_string()),
                                tool_metadata: Some(metadata),
                                token_usage: None,
                                model: None,
                                usage_hash: None,
                            });
                        }
                        "custom_tool_call" => {
                            let raw_name = payload
                                .get("name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("tool");
                            let input = payload.get("input").map(|v| {
                                if let Some(s) = v.as_str() {
                                    s.to_string()
                                } else {
                                    serde_json::to_string(v).unwrap_or_default()
                                }
                            });
                            let input_value =
                                codex_tool_input_value(raw_name, None, input.as_deref());
                            let metadata = build_tool_metadata(ToolCallFacts {
                                provider: Provider::Codex,
                                raw_name,
                                input: input_value.as_ref(),
                                call_id: payload.get("call_id").and_then(|v| v.as_str()),
                                assistant_id: None,
                            });
                            let display_name = metadata.canonical_name.clone();

                            let idx = messages.len();
                            if let Some(cid) = payload.get("call_id").and_then(|v| v.as_str()) {
                                call_id_map.insert(cid.to_string(), idx);
                            }
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: String::new(),
                                timestamp: entry.timestamp.clone(),
                                tool_name: Some(display_name.to_string()),
                                tool_input: input,
                                tool_metadata: Some(metadata),
                                token_usage: None,
                                model: None,
                                usage_hash: None,
                            });
                        }
                        "custom_tool_call_output" => {
                            let raw_output = payload
                                .get("output")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let output = extract_tool_output(&raw_output);

                            let call_id = payload.get("call_id").and_then(|v| v.as_str());
                            if let Some(idx) = call_id.and_then(|cid| call_id_map.get(cid)).copied()
                            {
                                if idx < messages.len() {
                                    let result_value =
                                        codex_tool_result_value(&raw_output, &output);
                                    messages[idx].content = output;
                                    let is_error = result_value.as_ref().and_then(|value| {
                                        value
                                            .get("exitCode")
                                            .and_then(|code| code.as_i64())
                                            .map(|code| code != 0)
                                    });
                                    if let Some(result_value) = result_value {
                                        enrich_existing_tool_message(
                                            &mut messages[idx],
                                            result_value,
                                            is_error,
                                            None,
                                        );
                                    }
                                    continue;
                                }
                            }
                            if !output.is_empty() {
                                messages.push(Message {
                                    role: MessageRole::Tool,
                                    content: output,
                                    timestamp: entry.timestamp.clone(),
                                    tool_name: None,
                                    tool_input: None,
                                    token_usage: None,
                                    model: None,
                                    usage_hash: None,
                                    tool_metadata: None,
                                });
                            }
                        }
                        _ => continue,
                    }
                }
                "turn_context" => {
                    flush_pending_user_message(
                        &mut pending_user_message,
                        &mut messages,
                        &mut content_parts,
                        &mut first_user_message,
                    );
                    // Extract actual model name (e.g. "gpt-5.4") from turn_context
                    if let Some(m) = payload.get("model").and_then(|v| v.as_str()) {
                        if !m.is_empty() {
                            current_model = Some(m.to_string());
                            if model.is_none() {
                                model = Some(m.to_string());
                            }
                        }
                    }
                }
                "event_msg" => {
                    let event_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                    // agent_message is a duplicate of response_item/message/assistant — skip
                    match event_type {
                        "user_message" => {
                            let pending = pending_user_message.take();
                            let fallback_content =
                                pending.as_ref().map(|message| message.content.clone());
                            let response_image_segments = pending
                                .as_ref()
                                .map(|message| message.image_segments.clone())
                                .unwrap_or_default();
                            let timestamp = entry
                                .timestamp
                                .clone()
                                .or_else(|| pending.and_then(|message| message.timestamp));
                            let built_content =
                                build_codex_user_message(payload, &response_image_segments);
                            let content = if built_content.is_empty() {
                                fallback_content.unwrap_or_default()
                            } else {
                                built_content
                            };
                            append_user_message(
                                &mut messages,
                                &mut content_parts,
                                &mut first_user_message,
                                content,
                                timestamp,
                            );
                        }
                        "token_count" => {
                            if let Some(info) = payload.get("info") {
                                if let Some(last) = info.get("last_token_usage") {
                                    let input = last
                                        .get("input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u32;
                                    let output = last
                                        .get("output_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u32;
                                    let cached = last
                                        .get("cached_input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0)
                                        as u32;
                                    let usage = TokenUsage {
                                        input_tokens: input,
                                        output_tokens: output,
                                        cache_read_input_tokens: cached,
                                        cache_creation_input_tokens: 0,
                                    };
                                    if let Some(last_msg) = messages
                                        .iter_mut()
                                        .rev()
                                        .find(|m| m.role == MessageRole::Assistant)
                                    {
                                        last_msg.token_usage = Some(usage);
                                    }
                                }
                            }
                        }
                        "web_search_end" => {
                            let call_id = payload.get("call_id").and_then(|v| v.as_str());
                            let action = payload.get("action");
                            let query = payload.get("query").and_then(|v| v.as_str()).unwrap_or("");
                            let result = Some(payload.clone());

                            if let Some(idx) = call_id.and_then(|cid| call_id_map.get(cid)).copied()
                            {
                                if idx < messages.len() {
                                    messages[idx].content = query.to_string();
                                    if messages[idx].tool_input.is_none() {
                                        messages[idx].tool_input =
                                            action.map(|value| value.to_string());
                                    }
                                    if let Some(metadata) = messages[idx].tool_metadata.as_mut() {
                                        enrich_tool_metadata(
                                            metadata,
                                            ToolResultFacts {
                                                raw_result: result.as_ref(),
                                                is_error: None,
                                                status: None,
                                                artifact_path: None,
                                            },
                                        );
                                    }
                                    continue;
                                }
                            }

                            let mut metadata = build_tool_metadata(ToolCallFacts {
                                provider: Provider::Codex,
                                raw_name: "web_search_call",
                                input: action,
                                call_id,
                                assistant_id: None,
                            });
                            enrich_tool_metadata(
                                &mut metadata,
                                ToolResultFacts {
                                    raw_result: result.as_ref(),
                                    is_error: None,
                                    status: None,
                                    artifact_path: None,
                                },
                            );
                            let idx = messages.len();
                            if let Some(call_id) = call_id {
                                call_id_map.insert(call_id.to_string(), idx);
                            }
                            if !query.is_empty() {
                                content_parts.push(query.to_string());
                            }
                            messages.push(Message {
                                role: MessageRole::Tool,
                                content: query.to_string(),
                                timestamp: entry.timestamp.clone(),
                                tool_name: Some(metadata.canonical_name.clone()),
                                tool_input: action.map(|value| value.to_string()),
                                tool_metadata: Some(metadata),
                                token_usage: None,
                                model: None,
                                usage_hash: None,
                            });
                        }
                        "exec_command_end" => {
                            let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str())
                            else {
                                continue;
                            };
                            let Some(idx) = call_id_map.get(call_id).copied() else {
                                log::warn!(
                                    "missing Codex exec_command tool message for event call_id {} in '{}'",
                                    call_id,
                                    path.display()
                                );
                                continue;
                            };
                            if idx >= messages.len() {
                                continue;
                            }

                            let result_value =
                                codex_exec_command_event_result(payload, &messages[idx].content);
                            let status = payload.get("status").and_then(|v| v.as_str());
                            let is_error =
                                status.map(|status| matches!(status, "failed" | "declined"));
                            if messages[idx].content.is_empty()
                                || messages[idx].content.trim_start().starts_with('{')
                            {
                                if let Some(formatted_output) = result_value
                                    .get("formattedOutput")
                                    .and_then(|v| v.as_str())
                                    .filter(|v| !v.is_empty())
                                    .or_else(|| {
                                        result_value
                                            .get("aggregatedOutput")
                                            .and_then(|v| v.as_str())
                                            .filter(|v| !v.is_empty())
                                    })
                                    .or_else(|| {
                                        result_value
                                            .get("stdout")
                                            .and_then(|v| v.as_str())
                                            .filter(|v| !v.is_empty())
                                    })
                                {
                                    messages[idx].content = formatted_output.to_string();
                                }
                            }
                            enrich_existing_tool_message(
                                &mut messages[idx],
                                result_value,
                                is_error,
                                status,
                            );
                        }
                        "mcp_tool_call_end" => {
                            let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str())
                            else {
                                continue;
                            };
                            let Some(idx) = call_id_map.get(call_id).copied() else {
                                log::warn!(
                                    "missing Codex MCP tool message for event call_id {} in '{}'",
                                    call_id,
                                    path.display()
                                );
                                continue;
                            };
                            if idx >= messages.len() {
                                continue;
                            }

                            let result_value = codex_mcp_tool_call_event_result(payload);
                            let is_error = result_value
                                .get("success")
                                .and_then(|v| v.as_bool())
                                .map(|success| !success);
                            enrich_existing_tool_message(
                                &mut messages[idx],
                                result_value,
                                is_error,
                                None,
                            );
                        }
                        "patch_apply_end" => {
                            let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str())
                            else {
                                continue;
                            };
                            let Some(idx) = call_id_map.get(call_id).copied() else {
                                log::warn!(
                                    "missing Codex apply_patch tool message for event call_id {} in '{}'",
                                    call_id,
                                    path.display()
                                );
                                continue;
                            };
                            if idx >= messages.len() {
                                continue;
                            }

                            let result_value = codex_patch_event_result(payload);
                            let status = payload.get("status").and_then(|v| v.as_str());
                            let is_error =
                                payload.get("success").and_then(|v| v.as_bool()).map(|v| !v);
                            if messages[idx].content.is_empty()
                                || messages[idx].content.trim_start().starts_with('{')
                            {
                                if let Some(stdout) = result_value
                                    .get("stdout")
                                    .and_then(|v| v.as_str())
                                    .filter(|v| !v.is_empty())
                                {
                                    messages[idx].content = stdout.to_string();
                                }
                            }
                            enrich_existing_tool_message(
                                &mut messages[idx],
                                result_value,
                                is_error,
                                status,
                            );
                        }
                        "collab_agent_spawn_end" => {
                            let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str())
                            else {
                                continue;
                            };
                            let Some(idx) = call_id_map.get(call_id).copied() else {
                                log::warn!(
                                    "missing Codex spawn_agent tool message for event call_id {} in '{}'",
                                    call_id,
                                    path.display()
                                );
                                continue;
                            };
                            if idx >= messages.len() {
                                continue;
                            }

                            let status = payload.get("status").and_then(|v| v.as_str());
                            enrich_existing_tool_message(
                                &mut messages[idx],
                                payload.clone(),
                                None,
                                status,
                            );
                        }
                        "collab_waiting_end" => {
                            let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str())
                            else {
                                continue;
                            };
                            let Some(idx) = call_id_map.get(call_id).copied() else {
                                log::warn!(
                                    "missing Codex wait_agent tool message for event call_id {} in '{}'",
                                    call_id,
                                    path.display()
                                );
                                continue;
                            };
                            if idx >= messages.len() {
                                continue;
                            }

                            let status = messages[idx]
                                .tool_metadata
                                .as_ref()
                                .and_then(|metadata| metadata.structured.as_ref())
                                .and_then(|value| value.get("timed_out"))
                                .and_then(|value| value.as_bool())
                                .map(|timed_out| if timed_out { "timed_out" } else { "completed" });
                            let is_error = status.map(|status| status == "timed_out");
                            enrich_existing_tool_message(
                                &mut messages[idx],
                                payload.clone(),
                                is_error,
                                status,
                            );
                        }
                        "collab_close_end" => {
                            let Some(call_id) = payload.get("call_id").and_then(|v| v.as_str())
                            else {
                                continue;
                            };
                            let Some(idx) = call_id_map.get(call_id).copied() else {
                                log::warn!(
                                    "missing Codex close_agent tool message for event call_id {} in '{}'",
                                    call_id,
                                    path.display()
                                );
                                continue;
                            };
                            if idx >= messages.len() {
                                continue;
                            }

                            enrich_existing_tool_message(
                                &mut messages[idx],
                                payload.clone(),
                                None,
                                Some("completed"),
                            );
                        }
                        _ => {
                            flush_pending_user_message(
                                &mut pending_user_message,
                                &mut messages,
                                &mut content_parts,
                                &mut first_user_message,
                            );
                        }
                    }
                }
                _ => continue,
            }
        }

        flush_pending_user_message(
            &mut pending_user_message,
            &mut messages,
            &mut content_parts,
            &mut first_user_message,
        );

        if messages.is_empty() {
            return None;
        }

        // Session ID: from session_meta payload.id, fallback to filename parsing
        let session_id = session_id.unwrap_or_else(|| {
            path.file_stem().map_or_else(
                || "unknown".to_string(),
                |s| s.to_string_lossy().to_string(),
            )
        });

        let title = agent_nickname
            .as_deref()
            .map(|n| n.to_string())
            .unwrap_or_else(|| session_title(first_user_message.as_deref()));

        let project_path = cwd.unwrap_or_else(|| NO_PROJECT.to_string());

        let project_name = project_name_from_path(&project_path);

        let created_at = parse_rfc3339_timestamp(first_timestamp.as_deref());

        let updated_at = parse_rfc3339_timestamp(last_timestamp.as_deref());

        let full_content = content_parts.join("\n");
        let content_text = truncate_to_bytes(&full_content, FTS_CONTENT_LIMIT);

        let meta = SessionMeta {
            id: session_id,
            provider: Provider::Codex,
            title,
            project_path,
            project_name,
            created_at,
            updated_at,
            message_count: messages.len() as u32,
            file_size_bytes: file_size,
            source_path: path.to_string_lossy().to_string(),
            is_sidechain,
            variant_name: None,
            model: model.or(model_provider),
            cc_version,
            git_branch,
            parent_id,
        };

        Some(ParsedSession {
            meta,
            messages,
            content_text,
            parse_warning_count: 0,
        })
    }
}

pub fn extract_usage_events_from_file(path: &PathBuf) -> Vec<CodexUsageEvent> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) => {
            log::warn!(
                "failed to open Codex session for usage extraction '{}': {}",
                path.display(),
                error
            );
            return Vec::new();
        }
    };
    let reader = BufReader::new(file);

    let mut current_model: Option<String> = None;
    let mut previous_totals: Option<(u64, u64, u64, u64, u64)> = None;
    let mut events = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(line) if !line.trim().is_empty() => line,
            Ok(_) => continue,
            Err(error) => {
                log::warn!(
                    "failed to read Codex usage line from '{}': {}",
                    path.display(),
                    error
                );
                continue;
            }
        };

        let entry: CodexLine = match serde_json::from_str(&line) {
            Ok(entry) => entry,
            Err(error) => {
                log::warn!(
                    "skipping malformed Codex usage JSONL in '{}': {}",
                    path.display(),
                    error
                );
                continue;
            }
        };

        let Some(payload) = entry.payload.as_ref() else {
            continue;
        };

        match entry.line_type.as_str() {
            "turn_context" => {
                current_model = extract_codex_model(payload).or(current_model);
            }
            "event_msg" => {
                if payload.get("type").and_then(|v| v.as_str()) != Some("token_count") {
                    continue;
                }
                let Some(timestamp) = entry.timestamp.clone() else {
                    continue;
                };
                let Some(info) = payload.get("info") else {
                    continue;
                };

                let last_usage = info
                    .get("last_token_usage")
                    .and_then(normalize_codex_raw_usage);
                let total_usage = info
                    .get("total_token_usage")
                    .and_then(normalize_codex_raw_usage);

                let raw_usage = match (last_usage, total_usage) {
                    (Some(last), _) => {
                        previous_totals = Some(last.1);
                        Some(last)
                    }
                    (None, Some(total)) => {
                        let delta = subtract_codex_usage(total.1, previous_totals);
                        previous_totals = Some(total.1);
                        Some((total.0, delta))
                    }
                    (None, None) => None,
                };

                let Some((model, (input, cached, output, reasoning, total))) = raw_usage else {
                    continue;
                };
                if input == 0 && cached == 0 && output == 0 && reasoning == 0 && total == 0 {
                    continue;
                }

                let model = extract_codex_model(info)
                    .or_else(|| extract_codex_model(payload))
                    .or_else(|| current_model.clone())
                    .unwrap_or_else(|| model.unwrap_or_else(|| "gpt-5".to_string()));
                current_model = Some(model.clone());

                let cache_read = cached.min(input);
                let non_cached_input = input.saturating_sub(cache_read);

                events.push(CodexUsageEvent {
                    timestamp,
                    model,
                    input_tokens: non_cached_input,
                    output_tokens: output,
                    cache_read_input_tokens: cache_read,
                });
            }
            _ => {}
        }
    }

    events
}

fn extract_codex_model(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            value
                .get("info")
                .and_then(|info| info.get("model"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .get("info")
                .and_then(|info| info.get("model_name"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| {
            value
                .get("metadata")
                .and_then(|meta| meta.get("model"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
}

type RawCodexUsage = (Option<String>, (u64, u64, u64, u64, u64));

fn normalize_codex_raw_usage(value: &Value) -> Option<RawCodexUsage> {
    let input = value
        .get("input_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let cached = value
        .get("cached_input_tokens")
        .or_else(|| value.get("cache_read_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let output = value
        .get("output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reasoning = value
        .get("reasoning_output_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = value
        .get("total_tokens")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| input + output);
    let model = value
        .get("model")
        .or_else(|| value.get("model_name"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    Some((model, (input, cached, output, reasoning, total)))
}

fn subtract_codex_usage(
    current: (u64, u64, u64, u64, u64),
    previous: Option<(u64, u64, u64, u64, u64)>,
) -> (u64, u64, u64, u64, u64) {
    let prev = previous.unwrap_or((0, 0, 0, 0, 0));
    (
        current.0.saturating_sub(prev.0),
        current.1.saturating_sub(prev.1),
        current.2.saturating_sub(prev.2),
        current.3.saturating_sub(prev.3),
        current.4.saturating_sub(prev.4),
    )
}

fn append_user_message(
    messages: &mut Vec<Message>,
    content_parts: &mut Vec<String>,
    first_user_message: &mut Option<String>,
    content: String,
    timestamp: Option<String>,
) {
    if content.is_empty() {
        return;
    }

    let normalized_text = strip_inline_image_sources(&content);
    let trimmed = normalized_text.trim_start();
    if is_system_content(trimmed) {
        return;
    }

    if first_user_message.is_none() {
        *first_user_message = Some(normalized_text.clone());
    }

    if !normalized_text.is_empty() {
        content_parts.push(normalized_text);
    }

    messages.push(Message {
        role: MessageRole::User,
        content,
        timestamp,
        tool_name: None,
        tool_input: None,
        token_usage: None,
        model: None,
        usage_hash: None,
        tool_metadata: None,
    });
}

fn flush_pending_user_message(
    pending_user_message: &mut Option<PendingCodexUserMessage>,
    messages: &mut Vec<Message>,
    content_parts: &mut Vec<String>,
    first_user_message: &mut Option<String>,
) {
    let Some(pending) = pending_user_message.take() else {
        return;
    };

    append_user_message(
        messages,
        content_parts,
        first_user_message,
        pending.content,
        pending.timestamp,
    );
}

#[cfg(test)]
mod tests {
    use super::extract_usage_events_from_file;
    use super::CodexProvider;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn extract_usage_events_splits_cached_input_from_input() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("codex.jsonl");
        fs::write(
            &file,
            concat!(
                "{\"timestamp\":\"2026-04-10T10:00:00Z\",\"type\":\"turn_context\",\"payload\":{\"model\":\"gpt-5.4\"}}\n",
                "{\"timestamp\":\"2026-04-10T10:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\",\"info\":{\"last_token_usage\":{\"input_tokens\":1000,\"cached_input_tokens\":600,\"output_tokens\":50,\"reasoning_output_tokens\":25,\"total_tokens\":1050}}}}\n"
            ),
        )
        .unwrap();

        let events = extract_usage_events_from_file(&file);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].model, "gpt-5.4");
        assert_eq!(events[0].input_tokens, 400);
        assert_eq!(events[0].cache_read_input_tokens, 600);
        assert_eq!(events[0].output_tokens, 50);
    }

    #[test]
    fn parse_session_file_emits_tool_metadata_for_web_search_end_event() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("codex.jsonl");
        fs::write(
            &file,
            concat!(
                "{\"timestamp\":\"2026-04-10T10:00:00Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"search docs\"}}\n",
                "{\"timestamp\":\"2026-04-10T10:00:01Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"web_search_end\",\"call_id\":\"ws_123\",\"query\":\"notify kqueue\",\"action\":{\"type\":\"search\",\"query\":\"notify kqueue\"}}}\n"
            ),
        )
        .unwrap();

        let provider = CodexProvider {
            home_dir: PathBuf::from("/tmp"),
        };
        let parsed = provider.parse_session_file(&file).expect("parsed session");
        let tool = parsed
            .messages
            .iter()
            .find(|message| message.tool_metadata.is_some())
            .expect("web search tool message");
        let metadata = tool.tool_metadata.as_ref().expect("tool metadata");

        assert_eq!(tool.tool_name.as_deref(), Some("WebSearch"));
        assert_eq!(tool.content, "notify kqueue");
        assert_eq!(metadata.raw_name, "web_search_call");
        assert_eq!(metadata.canonical_name, "WebSearch");
        assert_eq!(metadata.status.as_deref(), Some("success"));
        assert_eq!(metadata.summary.as_deref(), Some("notify kqueue"));
        assert_eq!(
            metadata
                .structured
                .as_ref()
                .and_then(|value| value.get("action"))
                .and_then(|value| value.get("query"))
                .and_then(|value| value.as_str()),
            Some("notify kqueue")
        );
    }

    #[test]
    fn parse_session_file_merges_exec_command_end_into_existing_tool_message() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("codex.jsonl");
        fs::write(
            &file,
            concat!(
                "{\"timestamp\":\"2026-04-10T10:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"exec_command\",\"arguments\":\"{\\\"cmd\\\":\\\"pwd\\\"}\",\"call_id\":\"exec_123\"}}\n",
                "{\"timestamp\":\"2026-04-10T10:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"exec_123\",\"output\":\"{\\\"output\\\":\\\"/tmp/project\\n\\\",\\\"metadata\\\":{\\\"exit_code\\\":0,\\\"duration_seconds\\\":0.2}}\"}}\n",
                "{\"timestamp\":\"2026-04-10T10:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"exec_command_end\",\"call_id\":\"exec_123\",\"process_id\":\"42\",\"turn_id\":\"turn_1\",\"command\":[\"pwd\"],\"cwd\":\"/tmp/project\",\"parsed_cmd\":[],\"source\":\"agent\",\"stdout\":\"/tmp/project\\n\",\"stderr\":\"\",\"aggregated_output\":\"/tmp/project\\n\",\"exit_code\":0,\"duration\":{\"secs\":1,\"nanos\":500000000},\"formatted_output\":\"/tmp/project\\n\",\"status\":\"completed\"}}\n"
            ),
        )
        .unwrap();

        let provider = CodexProvider {
            home_dir: PathBuf::from("/tmp"),
        };
        let parsed = provider.parse_session_file(&file).expect("parsed session");
        let tool = parsed
            .messages
            .iter()
            .find(|message| message.tool_name.as_deref() == Some("Bash"))
            .expect("bash tool message");
        let metadata = tool.tool_metadata.as_ref().expect("tool metadata");

        assert_eq!(tool.content, "/tmp/project\n");
        assert_eq!(metadata.status.as_deref(), Some("completed"));
        assert_eq!(metadata.result_kind.as_deref(), Some("terminal_output"));
        assert_eq!(
            metadata
                .structured
                .as_ref()
                .and_then(|value| value.get("cwd"))
                .and_then(|value| value.as_str()),
            Some("/tmp/project")
        );
        assert_eq!(
            metadata
                .structured
                .as_ref()
                .and_then(|value| value.get("source"))
                .and_then(|value| value.as_str()),
            Some("agent")
        );
        assert_eq!(
            metadata
                .structured
                .as_ref()
                .and_then(|value| value.get("exitCode"))
                .and_then(|value| value.as_i64()),
            Some(0)
        );
        assert_eq!(
            metadata
                .structured
                .as_ref()
                .and_then(|value| value.get("durationSeconds"))
                .and_then(|value| value.as_f64()),
            Some(1.5)
        );
    }

    #[test]
    fn parse_session_file_merges_patch_apply_end_into_existing_tool_message() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("codex.jsonl");
        fs::write(
            &file,
            concat!(
                "{\"timestamp\":\"2026-04-10T10:00:00Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call\",\"status\":\"completed\",\"call_id\":\"patch_123\",\"name\":\"apply_patch\",\"input\":\"*** Begin Patch\\n*** Update File: src/file.rs\\n@@\\n-old\\n+new\\n*** End Patch\\n\"}}\n",
                "{\"timestamp\":\"2026-04-10T10:00:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call_output\",\"call_id\":\"patch_123\",\"output\":\"{\\\"output\\\":\\\"Success. Updated the following files:\\\\nM src/file.rs\\\\n\\\",\\\"metadata\\\":{\\\"exit_code\\\":0,\\\"duration_seconds\\\":0.0}}\"}}\n",
                "{\"timestamp\":\"2026-04-10T10:00:02Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"patch_apply_end\",\"call_id\":\"patch_123\",\"turn_id\":\"turn_1\",\"stdout\":\"Success. Updated the following files:\\nM src/file.rs\\n\",\"stderr\":\"\",\"success\":true,\"changes\":{\"src/file.rs\":{\"type\":\"update\",\"unified_diff\":\"@@ -1 +1 @@\\n-old\\n+new\\n\",\"move_path\":null}},\"status\":\"completed\"}}\n"
            ),
        )
        .unwrap();

        let provider = CodexProvider {
            home_dir: PathBuf::from("/tmp"),
        };
        let parsed = provider.parse_session_file(&file).expect("parsed session");
        let tool = parsed
            .messages
            .iter()
            .find(|message| message.tool_name.as_deref() == Some("Edit"))
            .expect("apply patch tool message");
        let metadata = tool.tool_metadata.as_ref().expect("tool metadata");

        assert_eq!(metadata.status.as_deref(), Some("completed"));
        assert_eq!(metadata.result_kind.as_deref(), Some("file_patch"));
        assert_eq!(
            metadata
                .structured
                .as_ref()
                .and_then(|value| value.get("diff"))
                .and_then(|value| value.as_str())
                .map(|value| value.contains("*** Update File: src/file.rs")),
            Some(true)
        );
        assert_eq!(
            metadata
                .structured
                .as_ref()
                .and_then(|value| value.get("patches"))
                .and_then(|value| value.as_array())
                .and_then(|patches| patches.first())
                .and_then(|patch| patch.get("files"))
                .and_then(|value| value.as_array())
                .and_then(|files| files.first())
                .and_then(|value| value.as_str()),
            Some("src/file.rs")
        );
    }
}
