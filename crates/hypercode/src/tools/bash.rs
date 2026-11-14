//! `bash` tool: executes a shell command in the workspace root using `ToolContext.command_runner`
//! (e.g. NVIDIA OpenShell sandbox or sanitized host runner) with timeout and output truncation.

use crate::sandbox::{CommandRunner, HostRunner};
use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use hyperinfer_core::ToolDefinition;
use serde::Deserialize;
use std::time::Duration;

const MAX_OUTPUT_BYTES: usize = 32 * 1024;

pub struct BashTool;

#[derive(Deserialize)]
struct BashArgs {
    command: String,
    timeout_secs: Option<u64>,
    #[serde(default)]
    bypass_sandbox: bool,
}

fn truncate_output(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n... [output truncated at {} bytes]",
        &s[..end],
        max_bytes
    )
}

#[async_trait]
impl Tool for BashTool {
    fn name(&self) -> &str {
        "bash"
    }

    fn requires_approval(&self) -> bool {
        true
    }

    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "bash".to_string(),
            description: "Execute a bash command in the workspace root directory. Commands run inside the configured sandbox (such as NVIDIA OpenShell) unless `bypass_sandbox` is explicitly requested and approved.".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command line to execute."
                    },
                    "timeout_secs": {
                        "type": "integer",
                        "description": "Optional timeout in seconds (defaults to context timeout, typically 30s)."
                    },
                    "bypass_sandbox": {
                        "type": "boolean",
                        "description": "Set to true only if the command failed due to sandbox restrictions and requires explicit user approval to run unsandboxed."
                    }
                },
                "required": ["command"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput {
        let parsed: BashArgs = match serde_json::from_value(args) {
            Ok(a) => a,
            Err(e) => return ToolOutput::error(format!("Invalid arguments for bash: {}", e)),
        };

        if parsed.command.trim().is_empty() {
            return ToolOutput::error("Error: `command` cannot be empty.");
        }

        let timeout = parsed
            .timeout_secs
            .map(|s| Duration::from_secs(s.clamp(1, 300)))
            .unwrap_or(ctx.command_timeout);

        let exec_res = if parsed.bypass_sandbox {
            HostRunner
                .run(
                    &parsed.command,
                    &ctx.workspace_root,
                    timeout,
                    &ctx.sandbox_policy,
                )
                .await
        } else {
            ctx.command_runner
                .run(
                    &parsed.command,
                    &ctx.workspace_root,
                    timeout,
                    &ctx.sandbox_policy,
                )
                .await
        };

        match exec_res {
            Err(err_msg) => ToolOutput::error(err_msg),
            Ok(exec) => {
                let mut combined = format!(
                    "Exit code: {} (runner: {})\n",
                    exec.exit_code,
                    if exec.sandboxed { "sandboxed" } else { "host" }
                );
                if !exec.stdout.is_empty() {
                    combined.push_str("--- stdout ---\n");
                    combined.push_str(&truncate_output(&exec.stdout, MAX_OUTPUT_BYTES));
                    if !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                }
                if !exec.stderr.is_empty() {
                    combined.push_str("--- stderr ---\n");
                    combined.push_str(&truncate_output(&exec.stderr, MAX_OUTPUT_BYTES));
                    if !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                }

                if exec.exit_code == 0 {
                    ToolOutput::success(combined)
                } else {
                    ToolOutput::error(combined)
                }
            }
        }
    }
}
