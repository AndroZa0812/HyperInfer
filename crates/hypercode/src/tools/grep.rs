//! `grep` tool: regex search across text files in the workspace.

use crate::tool::{Tool, ToolContext, ToolOutput};
use crate::tools::list_files::is_ignored_dir;
use async_trait::async_trait;
use hyperinfer_core::ToolDefinition;
use regex::Regex;
use serde::Deserialize;
use std::path::Path;

const DEFAULT_MAX_RESULTS: usize = 50;
const MAX_FILE_SIZE_BYTES: u64 = 1024 * 1024; // 1 MB
const MAX_GREP_DEPTH: usize = 20;

pub struct GrepTool;

#[derive(Deserialize)]
struct GrepArgs {
    pattern: String,
    #[serde(default = "default_dot")]
    path: String,
    #[serde(default = "default_max_results")]
    max_results: usize,
}

fn default_dot() -> String {
    ".".to_string()
}

fn default_max_results() -> usize {
    DEFAULT_MAX_RESULTS
}

fn search_file(
    file_path: &Path,
    re: &Regex,
    ctx: &ToolContext,
    max_results: usize,
    matches: &mut Vec<String>,
) {
    if matches.len() >= max_results {
        return;
    }
    let meta = match std::fs::metadata(file_path) {
        Ok(m) => m,
        Err(_) => return,
    };
    if meta.len() > MAX_FILE_SIZE_BYTES {
        return;
    }
    let content = match std::fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(_) => return, // Skip binary / non-UTF8 files
    };

    let rel = ctx.to_relative_display(file_path);
    for (idx, line) in content.lines().enumerate() {
        if re.is_match(line) {
            matches.push(format!("{}:{}: {}", rel, idx + 1, line));
            if matches.len() >= max_results {
                break;
            }
        }
    }
}

fn search_path(
    target: &Path,
    re: &Regex,
    ctx: &ToolContext,
    depth: usize,
    max_results: usize,
    matches: &mut Vec<String>,
) {
    if depth > MAX_GREP_DEPTH || matches.len() >= max_results {
        return;
    }
    if target.is_file() {
        search_file(target, re, ctx, max_results, matches);
        return;
    }

    let read_dir = match std::fs::read_dir(target) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let mut entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if matches.len() >= max_results {
            break;
        }
        let is_symlink = entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false);
        if is_symlink {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if path.is_dir() {
            if is_ignored_dir(&name_str) {
                continue;
            }
            search_path(&path, re, ctx, depth + 1, max_results, matches);
        } else if path.is_file() {
            search_file(&path, re, ctx, max_results, matches);
        }
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "grep".to_string(),
            description: "Search for a regular expression pattern across files in the workspace, returning matching file paths, line numbers, and line contents.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regular expression pattern to search for."
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional directory or file path within the workspace to restrict the search (defaults to '.')."
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Maximum number of matching lines to return (defaults to 50)."
                    }
                },
                "required": ["pattern"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let parsed: GrepArgs = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return ToolOutput::error(format!("Invalid arguments for grep: {}", e)),
        };

        let re = match Regex::new(&parsed.pattern) {
            Ok(r) => r,
            Err(e) => {
                return ToolOutput::error(format!(
                    "Invalid regular expression '{}': {}",
                    parsed.pattern, e
                ));
            }
        };

        let resolved = match ctx.resolve_path(&parsed.path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if !resolved.exists() {
            return ToolOutput::error(format!("Path '{}' does not exist.", parsed.path));
        }

        let max_results = parsed.max_results.clamp(1, 500);
        let ctx_clone = ctx.clone();
        let matches = match tokio::task::spawn_blocking(move || {
            let mut m = Vec::new();
            search_path(&resolved, &re, &ctx_clone, 0, max_results, &mut m);
            m
        })
        .await
        {
            Ok(m) => m,
            Err(e) => return ToolOutput::error(format!("grep worker task failed: {}", e)),
        };

        if matches.is_empty() {
            return ToolOutput::success(format!(
                "No matches found for pattern '{}' in '{}'.",
                parsed.pattern, parsed.path
            ));
        }

        let truncated = matches.len() >= max_results;
        let mut out = matches.join("\n");
        if truncated {
            out.push_str(&format!("\n... [truncated at {} matches]", max_results));
        }

        ToolOutput::success(out)
    }
}
