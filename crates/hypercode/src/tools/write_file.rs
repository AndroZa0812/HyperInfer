//! `write_file` tool: creates or overwrites a file in the workspace.

use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use hyperinfer_core::ToolDefinition;
use serde::Deserialize;

pub struct WriteFileTool;

#[derive(Deserialize)]
struct WriteFileArgs {
    path: String,
    content: String,
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Create a new file or overwrite an existing file in the workspace with the given content. Parent directories are created automatically.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative or workspace path of the file to write."
                    },
                    "content": {
                        "type": "string",
                        "description": "Full text content to write to the file."
                    }
                },
                "required": ["path", "content"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let parsed: WriteFileArgs = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return ToolOutput::error(format!("Invalid arguments for write_file: {}", e)),
        };

        let resolved = match ctx.resolve_writable_path(&parsed.path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        if let Some(parent) = resolved.parent() {
            if let Err(e) = tokio::fs::create_dir_all(parent).await {
                return ToolOutput::error(format!(
                    "Failed to create parent directories for '{}': {}",
                    parsed.path, e
                ));
            }
        }

        let line_count = parsed.content.lines().count();
        let byte_count = parsed.content.len();

        if let Err(e) = tokio::fs::write(&resolved, &parsed.content).await {
            return ToolOutput::error(format!("Failed to write '{}': {}", parsed.path, e));
        }

        ToolOutput::success(format!(
            "Wrote {} bytes ({} lines) to '{}'.",
            byte_count,
            line_count,
            ctx.to_relative_display(&resolved)
        ))
    }
}
