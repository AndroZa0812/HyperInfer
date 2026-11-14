//! `read_file` tool: reads a file with 1-based line numbers and optional line range.

use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use hyperinfer_core::ToolDefinition;
use serde::Deserialize;

const MAX_LINES_PER_READ: usize = 500;
const MAX_BYTES_PER_READ: usize = 32 * 1024;

pub struct ReadFileTool;

#[derive(Deserialize)]
struct ReadFileArgs {
    path: String,
    start_line: Option<usize>,
    end_line: Option<usize>,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file in the workspace with 1-based line numbers. Optionally specify start_line and end_line (inclusive) to inspect a slice of a large file.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Relative or workspace path of the file to read."
                    },
                    "start_line": {
                        "type": "integer",
                        "description": "1-based starting line number (inclusive). Defaults to 1."
                    },
                    "end_line": {
                        "type": "integer",
                        "description": "1-based ending line number (inclusive)."
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let parsed: ReadFileArgs = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return ToolOutput::error(format!("Invalid arguments for read_file: {}", e)),
        };

        let resolved = match ctx.resolve_path(&parsed.path) {
            Ok(p) => p,
            Err(e) => return ToolOutput::error(e),
        };

        let raw = match tokio::fs::read_to_string(&resolved).await {
            Ok(s) => s,
            Err(e) => {
                return ToolOutput::error(format!("Failed to read '{}': {}", parsed.path, e));
            }
        };

        let lines: Vec<&str> = raw.lines().collect();
        let total_lines = lines.len();
        if total_lines == 0 {
            return ToolOutput::success(format!("File '{}' is empty (0 lines).", parsed.path));
        }

        let start = parsed.start_line.unwrap_or(1).max(1);
        if start > total_lines {
            return ToolOutput::error(format!(
                "start_line ({}) exceeds total lines ({}) in '{}'",
                start, total_lines, parsed.path
            ));
        }

        let requested_end = parsed
            .end_line
            .unwrap_or(start + MAX_LINES_PER_READ - 1)
            .min(total_lines);

        if requested_end < start {
            return ToolOutput::error(format!(
                "end_line ({}) cannot be less than start_line ({})",
                requested_end, start
            ));
        }

        let capped_end = requested_end.min(start + MAX_LINES_PER_READ - 1);
        let mut out = String::new();
        let mut actual_end = start;

        for (idx, line) in lines[(start - 1)..capped_end].iter().enumerate() {
            let line_no = start + idx;
            let formatted = format!("{:>4}: {}\n", line_no, line);
            if out.len() + formatted.len() > MAX_BYTES_PER_READ && !out.is_empty() {
                break;
            }
            out.push_str(&formatted);
            actual_end = line_no;
        }

        let header = format!(
            "Showing lines {}-{} of {} ({}):\n",
            start,
            actual_end,
            total_lines,
            ctx.to_relative_display(&resolved)
        );
        let mut result = header;
        result.push_str(&out);
        if actual_end < requested_end || capped_end < total_lines && parsed.end_line.is_none() {
            result.push_str(&format!(
                "... [truncated at line {}; pass start_line={} to read more]",
                actual_end,
                actual_end + 1
            ));
        }

        ToolOutput::success(result)
    }
}
