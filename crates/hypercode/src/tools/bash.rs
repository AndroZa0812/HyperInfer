//! `bash` tool: executes a shell command in the workspace root using `ToolContext.command_runner`
//! (e.g. NVIDIA OpenShell sandbox or sanitized host runner) with timeout and output truncation.

use crate::output_filter::{maybe_wrap_with_rtk, OutputFilter};
use crate::sandbox::{CommandRunner, HostRunner};
use crate::tool::{Tool, ToolContext, ToolOutput};
use async_trait::async_trait;
use hyperinfer_core::ToolDefinition;
use serde::Deserialize;
use std::time::Duration;

pub struct BashTool;

#[derive(Deserialize)]
struct BashArgs {
    command: String,
    timeout_secs: Option<u64>,
    #[serde(default)]
    bypass_sandbox: bool,
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

        let actual_command = maybe_wrap_with_rtk(&parsed.command, &ctx.output_filter_config)
            .unwrap_or_else(|| parsed.command.clone());

        let exec_res = if parsed.bypass_sandbox {
            HostRunner
                .run(
                    &actual_command,
                    &ctx.workspace_root,
                    timeout,
                    &ctx.sandbox_policy,
                )
                .await
        } else {
            ctx.command_runner
                .run(
                    &actual_command,
                    &ctx.workspace_root,
                    timeout,
                    &ctx.sandbox_policy,
                )
                .await
        };

        match exec_res {
            Err(err_msg) => ToolOutput::error(err_msg),
            Ok(exec) => {
                let filter = OutputFilter::new(ctx.output_filter_config.clone());
                let outcome = filter.filter_and_tee(
                    &parsed.command,
                    exec.exit_code,
                    &exec.stdout,
                    &exec.stderr,
                    &ctx.workspace_root,
                );
                ctx.filter_stats.record(&outcome.stats);

                let mut combined = format!(
                    "Exit code: {} (runner: {})\n",
                    exec.exit_code,
                    if exec.sandboxed { "sandboxed" } else { "host" }
                );
                if !outcome.output.is_empty() {
                    combined.push_str(&outcome.output);
                    if !combined.ends_with('\n') {
                        combined.push('\n');
                    }
                }
                if let Some(log_path) = &outcome.tee_log_path {
                    let rel = ctx.to_relative_display(log_path);
                    combined.push_str(&format!(
                        "\n[full raw output saved to {} — use read_file to inspect]",
                        rel
                    ));
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
