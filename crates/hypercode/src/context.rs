//! Workspace context discovery (`AGENTS.md` / `CLAUDE.md`), repo tree snapshots,
//! token estimation, and two-tier conversation compaction.

use crate::agent::{AgentError, ChatBackend};
use crate::tools::list_files::is_ignored_dir;
use hyperinfer_core::{ChatMessage, ChatRequest, MessageRole, Usage};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Candidate project rule/instruction files searched in `workspace_root`.
pub const INSTRUCTION_FILES: &[&str] = &["AGENTS.md", "CLAUDE.md", ".hypercode.md", ".cursorrules"];

const MAX_INSTRUCTION_BYTES: usize = 16 * 1024;
const MAX_TREE_ENTRIES: usize = 80;

/// Discovered project metadata used to enrich the agent's system prompt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceContext {
    pub workspace_root: PathBuf,
    /// `(filename, content)` of discovered instruction files (e.g. `AGENTS.md`).
    pub instruction_files: Vec<(String, String)>,
    /// Shallow directory tree of the workspace.
    pub file_tree: Vec<String>,
}

impl WorkspaceContext {
    /// Scan `workspace_root` for instruction files and build a shallow file tree.
    pub fn discover(workspace_root: impl Into<PathBuf>) -> Self {
        let root = workspace_root.into();
        let canonical = std::fs::canonicalize(&root).unwrap_or(root);

        let mut instruction_files = Vec::new();
        for &candidate in INSTRUCTION_FILES {
            let path = canonical.join(candidate);
            if path.is_file() {
                if let Ok(raw) = std::fs::read_to_string(&path) {
                    let trimmed = if raw.len() > MAX_INSTRUCTION_BYTES {
                        let mut end = MAX_INSTRUCTION_BYTES;
                        while !raw.is_char_boundary(end) {
                            end -= 1;
                        }
                        format!("{}\n... [truncated]", &raw[..end])
                    } else {
                        raw
                    };
                    instruction_files.push((candidate.to_string(), trimmed));
                }
            }
        }

        let mut file_tree = Vec::new();
        collect_shallow_tree(&canonical, &canonical, 1, 2, &mut file_tree);

        Self {
            workspace_root: canonical,
            instruction_files,
            file_tree,
        }
    }

    /// Build a complete system prompt combining `base_prompt`, workspace tree,
    /// and any discovered project instruction files.
    pub fn build_system_prompt(&self, base_prompt: &str) -> String {
        let mut out = String::from(base_prompt.trim());
        out.push_str("\n\n## Workspace Environment\n");
        out.push_str(&format!(
            "- Operating System: {}\n- Workspace Root: `{}`\n",
            std::env::consts::OS,
            self.workspace_root.display()
        ));

        if !self.file_tree.is_empty() {
            out.push_str("\n### Workspace Layout (shallow snapshot)\n```text\n");
            for entry in &self.file_tree {
                out.push_str(entry);
                out.push('\n');
            }
            out.push_str("```\n");
        }

        if !self.instruction_files.is_empty() {
            out.push_str("\n## Project Instructions\n");
            for (name, content) in &self.instruction_files {
                out.push_str(&format!(
                    "<project_instructions source=\"{}\">\n{}\n</project_instructions>\n",
                    name,
                    content.trim()
                ));
            }
        }

        out
    }
}

fn collect_shallow_tree(
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) {
    if depth > max_depth || out.len() >= MAX_TREE_ENTRIES {
        return;
    }
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let mut entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if out.len() >= MAX_TREE_ENTRIES {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();

        let is_symlink = entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false);

        if path.is_dir() && !is_symlink {
            if is_ignored_dir(&name_str) {
                continue;
            }
            out.push(format!("{}/", rel));
            if depth < max_depth {
                collect_shallow_tree(root, &path, depth + 1, max_depth, out);
            }
        } else {
            out.push(rel);
        }
    }
}

/// Estimate the token count of a string using the standard ~4 chars/token heuristic.
pub fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Estimate the total token count across a slice of `ChatMessage`s.
pub fn estimate_messages_tokens(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .map(|m| {
            let content_tokens = estimate_tokens(&m.content);
            let calls_tokens = m
                .tool_calls
                .as_ref()
                .map(|calls| {
                    calls
                        .iter()
                        .map(|tc| estimate_tokens(&tc.name) + estimate_tokens(&tc.arguments) + 8)
                        .sum::<usize>()
                })
                .unwrap_or(0);
            content_tokens + calls_tokens + 4
        })
        .sum()
}

/// Model-specific context window and pricing profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelContextProfile {
    pub context_window: usize,
    pub input_cost_per_million: f64,
    pub output_cost_per_million: f64,
    pub cache_read_cost_per_million: f64,
    pub cache_creation_cost_per_million: f64,
}

/// Return the context window and pricing profile for a given model name.
pub fn model_context_profile(model: &str) -> ModelContextProfile {
    let lower = model.to_lowercase();
    if lower.starts_with("claude-opus") {
        ModelContextProfile {
            context_window: 200_000,
            input_cost_per_million: 15.0,
            output_cost_per_million: 75.0,
            cache_read_cost_per_million: 1.50,
            cache_creation_cost_per_million: 18.75,
        }
    } else if lower.starts_with("claude-haiku") {
        ModelContextProfile {
            context_window: 200_000,
            input_cost_per_million: 0.80,
            output_cost_per_million: 4.00,
            cache_read_cost_per_million: 0.08,
            cache_creation_cost_per_million: 1.00,
        }
    } else if lower.starts_with("claude") {
        // Sonnet default
        ModelContextProfile {
            context_window: 200_000,
            input_cost_per_million: 3.00,
            output_cost_per_million: 15.00,
            cache_read_cost_per_million: 0.30,
            cache_creation_cost_per_million: 3.75,
        }
    } else if lower.starts_with("gpt-4.1") {
        ModelContextProfile {
            context_window: 1_000_000,
            input_cost_per_million: 2.00,
            output_cost_per_million: 8.00,
            cache_read_cost_per_million: 0.50,
            cache_creation_cost_per_million: 2.00,
        }
    } else if lower.starts_with("gpt-4o-mini") {
        ModelContextProfile {
            context_window: 128_000,
            input_cost_per_million: 0.15,
            output_cost_per_million: 0.60,
            cache_read_cost_per_million: 0.075,
            cache_creation_cost_per_million: 0.15,
        }
    } else if lower.starts_with("gpt-4o")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
    {
        ModelContextProfile {
            context_window: 128_000,
            input_cost_per_million: 2.50,
            output_cost_per_million: 10.00,
            cache_read_cost_per_million: 1.25,
            cache_creation_cost_per_million: 2.50,
        }
    } else if lower.starts_with("gemini") {
        ModelContextProfile {
            context_window: 1_000_000,
            input_cost_per_million: 1.25,
            output_cost_per_million: 5.00,
            cache_read_cost_per_million: 0.3125,
            cache_creation_cost_per_million: 1.25,
        }
    } else {
        // Local models (qwen, llama, etc.)
        ModelContextProfile {
            context_window: 32_000,
            input_cost_per_million: 0.0,
            output_cost_per_million: 0.0,
            cache_read_cost_per_million: 0.0,
            cache_creation_cost_per_million: 0.0,
        }
    }
}

/// Configuration for automatic and manual context compaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompactionConfig {
    /// Trigger automatic compaction when estimated history tokens exceed this threshold.
    pub max_context_tokens: usize,
    /// Number of most recent non-system messages to preserve verbatim.
    pub keep_recent_messages: usize,
    /// Maximum character length for older `MessageRole::Tool` outputs during Tier-1 pruning.
    pub max_old_tool_output_chars: usize,
    /// Fraction of the model's context window to use as the effective compaction threshold.
    pub compaction_ratio: f64,
    /// If true, compute the effective max context tokens from the model's context window.
    pub auto_model_budget: bool,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            max_context_tokens: 32_000,
            keep_recent_messages: 6,
            max_old_tool_output_chars: 800,
            compaction_ratio: 0.75,
            auto_model_budget: true,
        }
    }
}

impl CompactionConfig {
    /// Compute the effective max context tokens based on the model's context window.
    pub fn effective_max_context_tokens(&self, model: &str) -> usize {
        if self.auto_model_budget {
            let profile = model_context_profile(model);
            ((profile.context_window as f64) * self.compaction_ratio) as usize
        } else {
            self.max_context_tokens
        }
    }
}

/// Token usage accounting with calibrated context tracking.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TokenAccounting {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    /// Total prompt tokens reported by the API on the last step (input + cache_read + cache_creation).
    pub last_prompt_tokens: Option<usize>,
    /// History length at the time of the last API response.
    pub last_history_len: usize,
}

impl TokenAccounting {
    /// Record usage from an API response and update calibration state.
    pub fn record_usage(&mut self, usage: &Usage, history_len: usize) {
        self.input_tokens += usage.input_tokens as u64;
        self.output_tokens += usage.output_tokens as u64;
        let cache_read = usage.cache_read_tokens.unwrap_or(0) as u64;
        let cache_creation = usage.cache_creation_tokens.unwrap_or(0) as u64;
        self.cache_read_tokens += cache_read;
        self.cache_creation_tokens += cache_creation;

        let prompt_tokens =
            usage.input_tokens as usize + cache_read as usize + cache_creation as usize;
        if prompt_tokens > 0 {
            self.last_prompt_tokens = Some(prompt_tokens);
            self.last_history_len = history_len;
        }
    }

    /// Estimate current context tokens using calibration from the last API response.
    pub fn calibrated_context_tokens(&self, history: &[ChatMessage]) -> usize {
        if let Some(base) = self.last_prompt_tokens {
            if history.len() >= self.last_history_len {
                let new_messages = &history[self.last_history_len..];
                return base + estimate_messages_tokens(new_messages);
            }
        }
        estimate_messages_tokens(history)
    }

    /// Compute context utilization as a percentage of the model's context window.
    pub fn context_utilization_pct(&self, model: &str, history: &[ChatMessage]) -> f64 {
        let profile = model_context_profile(model);
        let used = self.calibrated_context_tokens(history) as f64;
        let window = profile.context_window.max(1) as f64;
        (used / window) * 100.0
    }

    /// Estimate total USD cost based on accumulated token usage and model pricing.
    pub fn estimated_cost_usd(&self, model: &str) -> f64 {
        let profile = model_context_profile(model);
        let input_cost = (self.input_tokens as f64 / 1_000_000.0) * profile.input_cost_per_million;
        let output_cost =
            (self.output_tokens as f64 / 1_000_000.0) * profile.output_cost_per_million;
        let cache_read_cost =
            (self.cache_read_tokens as f64 / 1_000_000.0) * profile.cache_read_cost_per_million;
        let cache_creation_cost = (self.cache_creation_tokens as f64 / 1_000_000.0)
            * profile.cache_creation_cost_per_million;
        input_cost + output_cost + cache_read_cost + cache_creation_cost
    }
}

/// Deduplicate superseded `read_file` tool outputs in history.
///
/// Walks history to find `read_file` calls and tracks the latest index where each
/// file was read or mutated. Any earlier `read_file` results are replaced with a
/// superseded marker. Returns the number of messages deduplicated.
pub fn deduplicate_read_file_outputs(history: &mut [ChatMessage]) -> usize {
    // First pass: collect tool_call_id -> (assistant_idx, path) for read_file calls
    // and track the latest assistant index where each path was read or mutated.
    let mut call_paths: HashMap<String, (usize, String)> = HashMap::new();
    let mut latest_path_index: HashMap<String, usize> = HashMap::new();
    for (idx, msg) in history.iter().enumerate() {
        if msg.role == MessageRole::Assistant {
            if let Some(ref calls) = msg.tool_calls {
                for tc in calls {
                    if matches!(tc.name.as_str(), "read_file" | "edit_file" | "write_file") {
                        if let Ok(args) = serde_json::from_str::<serde_json::Value>(&tc.arguments) {
                            if let Some(path) = args.get("path").and_then(|p| p.as_str()) {
                                if tc.name == "read_file" {
                                    call_paths.insert(tc.id.clone(), (idx, path.to_string()));
                                }
                                latest_path_index.insert(path.to_string(), idx);
                            }
                        }
                    }
                }
            }
        }
    }

    // Second pass: replace superseded read_file results
    let mut deduped_count = 0;
    for msg in history.iter_mut() {
        if msg.role == MessageRole::Tool {
            if let Some(ref call_id) = msg.tool_call_id {
                if let Some((call_idx, path)) = call_paths.get(call_id) {
                    if let Some(&latest_idx) = latest_path_index.get(path) {
                        if *call_idx < latest_idx && !msg.content.starts_with("[Superseded:") {
                            msg.content = format!(
                                "[Superseded: `{}` was re-read or modified in a later step]",
                                path
                            );
                            deduped_count += 1;
                        }
                    }
                }
            }
        }
    }

    deduped_count
}

/// Bytes reserved for `\n... [N chars pruned by context compactor] ...\n`
/// so a pruned message stays strictly `<= max_chars` and is never re-pruned.
const PRUNE_MARKER_RESERVE: usize = 64;

/// Tier 1 Compaction: Prune oversized `MessageRole::Tool` outputs in older messages
/// while preserving all `ToolCall` / `tool_call_id` pairs and keeping the most recent
/// `keep_recent_messages` untouched.
///
/// Returns the number of tool messages pruned.
pub fn prune_old_tool_outputs(
    history: &mut [ChatMessage],
    keep_recent_messages: usize,
    max_chars: usize,
) -> usize {
    if history.len() <= keep_recent_messages {
        return 0;
    }

    let max_chars = max_chars.max(PRUNE_MARKER_RESERVE);
    let cutoff = history.len().saturating_sub(keep_recent_messages);
    let mut pruned_count = 0;

    for msg in &mut history[..cutoff] {
        if msg.role == MessageRole::Tool && msg.content.len() > max_chars {
            let half = max_chars.saturating_sub(PRUNE_MARKER_RESERVE) / 2;
            let mut head_end = half.min(msg.content.len());
            while !msg.content.is_char_boundary(head_end) {
                head_end -= 1;
            }
            let mut tail_start = msg.content.len().saturating_sub(half).max(head_end);
            while !msg.content.is_char_boundary(tail_start) {
                tail_start += 1;
            }
            let omitted = tail_start.saturating_sub(head_end);
            if omitted > 0 {
                msg.content = format!(
                    "{}\n... [{} chars pruned by context compactor] ...\n{}",
                    &msg.content[..head_end],
                    omitted,
                    &msg.content[tail_start..]
                );
                pruned_count += 1;
            }
        }
    }

    pruned_count
}

/// Find a safe split index in `history` for Tier-2 summarization so that:
/// 1. Leading `System` messages stay at the front.
/// 2. At least `keep_recent_messages` remain in the tail, AND the tail starts
///    cleanly on a `MessageRole::User` boundary (or, in a long single-turn tool loop,
///    on a `MessageRole::Assistant` boundary immediately after a completed `Tool` result
///    batch, never splitting an `Assistant` `tool_calls` message from its `Tool` results!).
pub fn find_safe_compaction_split(
    history: &[ChatMessage],
    keep_recent_messages: usize,
) -> Option<(usize, usize)> {
    let start_idx = history.iter().position(|m| m.role != MessageRole::System)?;

    if history.len().saturating_sub(start_idx) <= keep_recent_messages {
        return None;
    }

    let target_split = history.len().saturating_sub(keep_recent_messages);
    // Prefer splitting at a `User` turn boundary; if none exists (e.g. a single turn
    // with many tool steps), fall back to splitting right before an `Assistant` step.
    let split_idx = (start_idx + 1..=target_split)
        .rev()
        .find(|&idx| history[idx].role == MessageRole::User)
        .or_else(|| {
            (start_idx + 1..=target_split)
                .rev()
                .find(|&idx| history[idx].role == MessageRole::Assistant)
        })?;

    if split_idx <= start_idx {
        None
    } else {
        Some((start_idx, split_idx))
    }
}

/// Tier 2 Compaction (`/compact`): Summarize older conversation turns using the LLM
/// and replace them with a compact `<conversation_summary>` message.
pub async fn compact_history_with_llm(
    history: &mut Vec<ChatMessage>,
    keep_recent_messages: usize,
    backend: &dyn ChatBackend,
    api_key_id: &str,
    model: &str,
) -> Result<bool, AgentError> {
    let (start_idx, split_idx) = match find_safe_compaction_split(history, keep_recent_messages) {
        Some(bounds) => bounds,
        None => return Ok(false),
    };

    let slice_to_summarize = &history[start_idx..split_idx];
    let mut transcript = String::new();
    for msg in slice_to_summarize {
        let role_label = match msg.role {
            MessageRole::System => "SYSTEM",
            MessageRole::User => "USER",
            MessageRole::Assistant => "ASSISTANT",
            MessageRole::Tool => "TOOL",
        };
        transcript.push_str(&format!("[{}]: {}\n", role_label, msg.content));
        if let Some(ref calls) = msg.tool_calls {
            for tc in calls {
                transcript.push_str(&format!("  -> tool_call: {}({})\n", tc.name, tc.arguments));
            }
        }
    }

    let sum_request = ChatRequest {
        model: model.to_string(),
        messages: vec![
            ChatMessage::system(
                "Summarize the following coding session transcript concisely. Preserve: \
                 (1) user goals & active plan, (2) files modified or inspected, \
                 (3) key decisions & findings, and (4) unresolved errors & remaining work.",
            ),
            ChatMessage::user(transcript),
        ],
        temperature: Some(0.0),
        max_tokens: Some(1024),
        stream: None,
        stop: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        prompt_caching: None,
    };

    let resp = backend.chat(api_key_id, sum_request).await?;
    let summary_text = resp
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content)
        .unwrap_or_default();

    let split_starts_with_assistant = history
        .get(split_idx)
        .map(|m| m.role == MessageRole::Assistant)
        .unwrap_or(false);

    let mut summary_messages = vec![
        ChatMessage::user("[Previous conversation history compacted]"),
        ChatMessage::assistant(format!(
            "<conversation_summary>\n{}\n</conversation_summary>",
            summary_text.trim()
        )),
    ];
    if split_starts_with_assistant {
        summary_messages.push(ChatMessage::user(
            "[Continuing current task after context compaction]",
        ));
    }

    history.splice(start_idx..split_idx, summary_messages);
    Ok(true)
}
