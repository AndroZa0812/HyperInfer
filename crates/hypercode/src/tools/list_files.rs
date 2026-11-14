//! `list_files` tool: lists files and directories inside the workspace.

use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use hyperinfer_core::ToolDefinition;
use serde::Deserialize;
use std::path::Path;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    "dist",
    "build",
];

const MAX_ENTRIES: usize = 500;

pub struct ListFilesTool;

#[derive(Deserialize)]
struct ListFilesArgs {
    #[serde(default = "default_dot")]
    path: String,
    #[serde(default = "default_depth")]
    max_depth: usize,
}

fn default_dot() -> String {
    ".".to_string()
}

fn default_depth() -> usize {
    3
}

pub(crate) fn is_ignored_dir(name: &str) -> bool {
    IGNORED_DIRS.contains(&name)
}

fn collect_entries(
    dir: &Path,
    ctx: &ToolContext,
    current_depth: usize,
    max_depth: usize,
    out: &mut Vec<String>,
) {
    if current_depth > max_depth || out.len() >= MAX_ENTRIES {
        return;
    }

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    let mut entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if out.len() >= MAX_ENTRIES {
            break;
        }
        let is_symlink = entry.file_type().map(|ft| ft.is_symlink()).unwrap_or(false);
        let path = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();

        if path.is_dir() && !is_symlink {
            if is_ignored_dir(&name_str) {
                continue;
            }
            out.push(format!("{}/", ctx.to_relative_display(&path)));
            if current_depth < max_depth {
                collect_entries(&path, ctx, current_depth + 1, max_depth, out);
            }
        } else {
            out.push(ctx.to_relative_display(&path));
        }
    }
}

#[async_trait]
impl Tool for ListFilesTool {
    fn name(&self) -> &str {
        "list_files"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "list_files".to_string(),
            description: "List files and directories inside the workspace up to `max_depth`, automatically skipping build/cache directories (.git, target, node_modules).".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative directory path to list (defaults to '.')."
                    },
                    "max_depth": {
                        "type": "integer",
                        "description": "Maximum directory traversal depth (defaults to 3)."
                    }
                }
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let parsed: ListFilesArgs = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return ToolOutput::error(format!("Invalid arguments for list_files: {}", e)),
        };

        let resolved = match ctx.resolve_path(&parsed.path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if !resolved.exists() {
            return ToolOutput::error(format!("Directory '{}' does not exist.", parsed.path));
        }
        if !resolved.is_dir() {
            return ToolOutput::error(format!("'{}' is a file, not a directory.", parsed.path));
        }

        let max_depth = parsed.max_depth.clamp(1, 10);
        let ctx_clone = ctx.clone();
        let entries = match tokio::task::spawn_blocking(move || {
            let mut out = Vec::new();
            collect_entries(&resolved, &ctx_clone, 1, max_depth, &mut out);
            out
        })
        .await
        {
            Ok(out) => out,
            Err(e) => return ToolOutput::error(format!("list_files worker task failed: {}", e)),
        };

        if entries.is_empty() {
            return ToolOutput::success(format!("Directory '{}' is empty.", parsed.path));
        }

        let truncated = entries.len() >= MAX_ENTRIES;
        let mut body = entries.join("\n");
        if truncated {
            body.push_str(&format!("\n... [truncated at {} entries]", MAX_ENTRIES));
        }

        ToolOutput::success(body)
    }
}
