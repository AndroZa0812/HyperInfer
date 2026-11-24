//! Core Tool trait, execution context, and registry for HyperCode.

use crate::output_filter::{OutputFilterConfig, SessionFilterStats};
use crate::sandbox::{CommandRunner, HostRunner, SandboxPolicy};
use async_trait::async_trait;
use hyperinfer_core::{ToolCall, ToolDefinition};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Execution context passed to every tool invocation.
#[derive(Clone)]
pub struct ToolContext {
    /// Root directory of the workspace. Tools are confined to this directory.
    pub workspace_root: PathBuf,
    /// Default timeout for shell commands.
    pub command_timeout: Duration,
    /// Security and sandbox policy for command execution.
    pub sandbox_policy: SandboxPolicy,
    /// Pluggable command execution backend (e.g. `OpenShellRunner` or `HostRunner`).
    pub command_runner: Arc<dyn CommandRunner>,
    /// Output filtering configuration for bash tool results.
    pub output_filter_config: OutputFilterConfig,
    /// Session-wide filter statistics.
    pub filter_stats: Arc<SessionFilterStats>,
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("workspace_root", &self.workspace_root)
            .field("command_timeout", &self.command_timeout)
            .field("sandbox_policy", &self.sandbox_policy)
            .field("command_runner", &self.command_runner.name())
            .field("output_filter_config", &self.output_filter_config)
            .finish()
    }
}

impl ToolContext {
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        let root = workspace_root.into();
        let canonical = std::fs::canonicalize(&root).unwrap_or(root);
        let sandbox_policy = SandboxPolicy::restricted_for_workspace(&canonical);
        Self {
            workspace_root: canonical,
            command_timeout: Duration::from_secs(30),
            sandbox_policy,
            command_runner: Arc::new(HostRunner),
            output_filter_config: OutputFilterConfig::default(),
            filter_stats: Arc::new(SessionFilterStats::default()),
        }
    }

    pub fn with_command_timeout(mut self, timeout: Duration) -> Self {
        self.command_timeout = timeout;
        self
    }

    pub fn with_sandbox_policy(mut self, policy: SandboxPolicy) -> Self {
        self.sandbox_policy = policy;
        self
    }

    pub fn with_command_runner<R: CommandRunner + 'static>(mut self, runner: R) -> Self {
        self.command_runner = Arc::new(runner);
        self
    }

    pub fn with_output_filter_config(mut self, config: OutputFilterConfig) -> Self {
        self.output_filter_config = config;
        self
    }

    /// Resolve a user/model-supplied path relative to `workspace_root`, rejecting
    /// any path that escapes outside `workspace_root`.
    pub fn resolve_path(&self, raw_path: &str) -> Result<PathBuf, String> {
        let trimmed = raw_path.trim();
        if trimmed.is_empty() {
            return Err("Path cannot be empty".to_string());
        }

        let input_path = Path::new(trimmed);
        let joined = if input_path.is_absolute() {
            input_path.to_path_buf()
        } else {
            self.workspace_root.join(input_path)
        };

        // Normalize `..` and `.` components lexically first so non-existent files
        // (e.g. for `write_file`) can still be validated against `workspace_root`.
        let mut normalized = PathBuf::new();
        for comp in joined.components() {
            match comp {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !normalized.pop() {
                        return Err(format!("Path '{}' escapes workspace root", raw_path));
                    }
                }
                other => normalized.push(other.as_os_str()),
            }
        }

        // Walk up ancestors to find the deepest existing path on disk, canonicalize it
        // to resolve any intermediate symlinks (even when nested subdirectories do not
        // exist yet), and re-join the remaining non-existent tail components.
        let mut effective = normalized.clone();
        for ancestor in normalized.ancestors() {
            if ancestor.exists() {
                let canon_ancestor = std::fs::canonicalize(ancestor)
                    .map_err(|e| format!("Invalid path '{}': {}", raw_path, e))?;
                effective = match normalized.strip_prefix(ancestor) {
                    Ok(rel_tail) if !rel_tail.as_os_str().is_empty() => {
                        canon_ancestor.join(rel_tail)
                    }
                    _ => canon_ancestor,
                };
                break;
            }
        }

        if !effective.starts_with(&self.workspace_root) {
            return Err(format!(
                "Access denied: path '{}' resolves outside workspace root '{}'",
                raw_path,
                self.workspace_root.display()
            ));
        }

        Ok(effective)
    }

    /// Format an absolute workspace path as a clean relative path for display.
    pub fn to_relative_display(&self, path: &Path) -> String {
        path.strip_prefix(&self.workspace_root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }
}

/// Output returned by a tool execution.
///
/// Tool failures are captured with `is_error: true` rather than aborting the
/// agent loop so the LLM can inspect the error message and self-correct.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
}

impl ToolOutput {
    pub fn success(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: message.into(),
            is_error: true,
        }
    }
}

/// Trait implemented by every tool available to the HyperCode agent.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Unique tool identifier (e.g. `"read_file"`).
    fn name(&self) -> &str;

    /// Provider-agnostic JSON Schema definition sent to the LLM.
    fn definition(&self) -> ToolDefinition;

    /// Whether executing this tool mutates state or runs external commands,
    /// allowing an approval gate to prompt the user first.
    fn requires_approval(&self) -> bool {
        false
    }

    /// Execute the tool with parsed JSON arguments.
    async fn execute(&self, args: serde_json::Value, ctx: &ToolContext) -> ToolOutput;
}

/// Thread-safe registry of tools exposed to the agent.
#[derive(Clone, Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Create a registry pre-populated with HyperCode's standard coding tools:
    /// `read_file`, `write_file`, `edit_file`, `list_files`, `grep`, and `bash`.
    pub fn with_default_coding_tools() -> Self {
        let mut registry = Self::new();
        crate::tools::register_default_tools(&mut registry);
        registry
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Arc::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn contains(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Return all registered `ToolDefinition`s sorted deterministically by name.
    ///
    /// Deterministic ordering preserves LLM prompt prefix caching across turns.
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> = self.tools.values().map(|t| t.definition()).collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// Parse a `ToolCall`'s JSON arguments and execute the matching tool.
    pub async fn execute(&self, call: &ToolCall, ctx: &ToolContext) -> ToolOutput {
        let tool = match self.get(&call.name) {
            Some(t) => t,
            None => {
                return ToolOutput::error(format!(
                    "Error: unknown tool '{}'. Available tools: {}",
                    call.name,
                    self.available_tool_names().join(", ")
                ));
            }
        };

        let args: serde_json::Value = if call.arguments.trim().is_empty() {
            serde_json::json!({})
        } else {
            match serde_json::from_str(&call.arguments) {
                Ok(v) => v,
                Err(e) => {
                    return ToolOutput::error(format!(
                        "Error: invalid JSON arguments for tool '{}': {}",
                        call.name, e
                    ));
                }
            }
        };

        tool.execute(args, ctx).await
    }

    pub fn available_tool_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tools.keys().cloned().collect();
        names.sort();
        names
    }
}
