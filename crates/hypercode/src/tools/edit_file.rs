//! `edit_file` tool: surgical search-and-replace on an existing file.
//!
//! Enforces uniqueness of `target` so the LLM never accidentally corrupts
//! multiple occurrences when intending a single edit.

use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use hyperinfer_core::ToolDefinition;
use serde::Deserialize;

pub struct EditFileTool;

#[derive(Deserialize)]
struct EditFileArgs {
    path: String,
    target: String,
    replacement: String,
}

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "edit_file".to_string(),
            description: "Perform an exact search-and-replace edit on an existing file. `target` must match the existing file text (including indentation) uniquely.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative or workspace path of the file to edit."
                    },
                    "target": {
                        "type": "string",
                        "description": "Exact existing text block to replace (include enough surrounding lines to make it unique)."
                    },
                    "replacement": {
                        "type": "string",
                        "description": "New text block to substitute in place of `target`."
                    }
                },
                "required": ["path", "target", "replacement"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let parsed: EditFileArgs = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return ToolOutput::error(format!("Invalid arguments for edit_file: {}", e)),
        };

        if parsed.target.is_empty() {
            return ToolOutput::error("Error: `target` cannot be empty in edit_file.");
        }

        let resolved = match ctx.resolve_writable_path(&parsed.path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        let original = match tokio::fs::read_to_string(&resolved).await {
            Ok(s) => s,
            Err(e) => {
                return ToolOutput::error(format!("Failed to read '{}': {}", parsed.path, e));
            }
        };

        let match_count = original.matches(&parsed.target).count();
        if match_count == 0 {
            return ToolOutput::error(format!(
                "Error: `target` string was not found in '{}'. Use `read_file` to inspect the exact file contents and whitespace before retrying.",
                ctx.to_relative_display(&resolved)
            ));
        }
        if match_count > 1 {
            return ToolOutput::error(format!(
                "Error: `target` matched {} locations in '{}'. Include more surrounding context lines in `target` so it matches uniquely.",
                match_count,
                ctx.to_relative_display(&resolved)
            ));
        }

        let byte_offset = original.find(&parsed.target).unwrap_or(0);
        let start_line = original[..byte_offset]
            .bytes()
            .filter(|&b| b == b'\n')
            .count()
            + 1;
        let target_lines = parsed.target.lines().count();
        let replacement_lines = parsed.replacement.lines().count();

        let updated = original.replacen(&parsed.target, &parsed.replacement, 1);
        if let Err(e) = tokio::fs::write(&resolved, &updated).await {
            return ToolOutput::error(format!("Failed to write '{}': {}", parsed.path, e));
        }

        ToolOutput::success(format!(
            "Edited '{}' at line {} (-{} / +{} lines).",
            ctx.to_relative_display(&resolved),
            start_line,
            target_lines,
            replacement_lines
        ))
    }
}
