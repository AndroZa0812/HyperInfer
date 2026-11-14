//! The HyperCode Agent Loop (ReAct Engine) and Permission/Sandbox Gate.

use crate::context::{
    compact_history_with_llm, estimate_messages_tokens, prune_old_tool_outputs, CompactionConfig,
    WorkspaceContext,
};
use crate::tool::{ToolContext, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use hyperinfer_client::HyperInferClient;
use hyperinfer_core::{ChatMessage, ChatRequest, ChatResponse, HyperInferError, ToolCall, Usage};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;

/// Errors that can terminate an agent turn.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM provider error: {0}")]
    Llm(#[from] HyperInferError),

    #[error("Agent exceeded maximum step limit ({max_steps}) without completing turn")]
    MaxStepsExceeded { max_steps: usize },

    #[error("LLM returned an empty choices list")]
    EmptyResponse,
}

/// Controls when the agent must ask the `ApprovalHandler` before executing a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApprovalMode {
    /// Autonomous mode: approve all tool calls without prompting.
    AutoApprove,
    /// Smart sandbox mode (default):
    /// - Read-only tools (`read_file`, `list_files`, `grep`) are auto-approved.
    /// - `bash` commands running inside an isolated sandbox (e.g. `OpenShellRunner`)
    ///   without `bypass_sandbox: true` are auto-approved.
    /// - File mutations (`write_file`, `edit_file`), unsandboxed `bash`, or
    ///   `bypass_sandbox: true` require approval from the `ApprovalHandler`.
    #[default]
    SmartSandbox,
    /// Prompt the `ApprovalHandler` for any tool where `requires_approval()` is true.
    ApproveMutations,
    /// Prompt the `ApprovalHandler` before every tool call.
    ApproveAll,
}

/// Pluggable human-in-the-loop approval gate.
#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    /// Return `true` to allow the tool call or `false` to deny it.
    async fn approve(&self, call: &ToolCall, reason: &str) -> bool;
}

/// Approval handler that approves all requests.
#[derive(Debug, Clone, Default)]
pub struct AutoApproveHandler;

#[async_trait]
impl ApprovalHandler for AutoApproveHandler {
    async fn approve(&self, _call: &ToolCall, _reason: &str) -> bool {
        true
    }
}

/// Approval handler that denies all requests requiring approval.
#[derive(Debug, Clone, Default)]
pub struct RejectAllHandler;

#[async_trait]
impl ApprovalHandler for RejectAllHandler {
    async fn approve(&self, _call: &ToolCall, _reason: &str) -> bool {
        false
    }
}

/// Structured lifecycle events emitted during an agent turn for UI/CLI rendering and logging.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AgentEvent {
    TurnStarted {
        user_prompt: String,
    },
    StepStarted {
        step: usize,
    },
    AssistantResponse {
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolApprovalChecked {
        tool_call: ToolCall,
        reason: String,
        approved: bool,
    },
    ToolExecuted {
        tool_call: ToolCall,
        output: ToolOutput,
    },
    ContextCompacted {
        tokens_before: usize,
        tokens_after: usize,
    },
    TurnCompleted {
        final_response: String,
        steps_taken: usize,
        total_usage: Usage,
    },
}

/// Trait for receiving real-time `AgentEvent` notifications.
pub trait EventSink: Send + Sync {
    fn on_event(&self, event: &AgentEvent);
}

/// No-op event sink used by default.
#[derive(Debug, Clone, Default)]
pub struct NoopEventSink;

impl EventSink for NoopEventSink {
    fn on_event(&self, _event: &AgentEvent) {}
}

/// Abstracts the LLM client so `Agent` works seamlessly with `HyperInferClient`
/// as well as deterministic mock backends in tests.
#[async_trait]
pub trait ChatBackend: Send + Sync {
    async fn chat(&self, key: &str, request: ChatRequest) -> Result<ChatResponse, HyperInferError>;
}

#[async_trait]
impl ChatBackend for HyperInferClient {
    async fn chat(&self, key: &str, request: ChatRequest) -> Result<ChatResponse, HyperInferError> {
        HyperInferClient::chat(self, key, request).await
    }
}

/// Configuration for a `HyperCode` agent session.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Target model identifier (e.g. `"claude-sonnet-4-20250514"`, `"gpt-4o"`, `"gemini-2.5-pro"`, `"antigravity/gemini-2.5-pro"`).
    pub model: String,
    /// Virtual key / caller identifier passed to `HyperInferClient`.
    pub api_key_id: String,
    /// System instructions prepended to the conversation.
    pub system_prompt: String,
    /// Maximum number of LLM round-trips per user turn before aborting.
    pub max_steps: usize,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub approval_mode: ApprovalMode,
    pub compaction: CompactionConfig,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(),
            api_key_id: "hypercode-local".to_string(),
            system_prompt: concat!(
                "You are HyperCode, an expert AI coding assistant.\n",
                "Use the provided tools to inspect the codebase, make surgical edits, ",
                "and verify your changes before answering.\n",
                "Prefer `edit_file` over `write_file` when modifying existing files."
            )
            .to_string(),
            max_steps: 25,
            temperature: Some(0.0),
            max_tokens: Some(4096),
            approval_mode: ApprovalMode::SmartSandbox,
            compaction: CompactionConfig::default(),
        }
    }
}

/// Summary returned when `Agent::run_turn` completes.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnOutcome {
    pub final_response: String,
    pub steps_taken: usize,
    pub tool_calls_executed: usize,
    pub total_usage: Usage,
}

/// Stateful coding agent that manages conversation history and executes the ReAct tool loop.
pub struct Agent {
    config: AgentConfig,
    backend: Arc<dyn ChatBackend>,
    registry: ToolRegistry,
    tool_ctx: ToolContext,
    approval_handler: Arc<dyn ApprovalHandler>,
    event_sink: Arc<dyn EventSink>,
    history: Vec<ChatMessage>,
}

impl Agent {
    pub fn new(
        config: AgentConfig,
        backend: Arc<dyn ChatBackend>,
        registry: ToolRegistry,
        tool_ctx: ToolContext,
    ) -> Self {
        let mut history = Vec::new();
        if !config.system_prompt.trim().is_empty() {
            history.push(ChatMessage::system(config.system_prompt.clone()));
        }
        Self {
            config,
            backend,
            registry,
            tool_ctx,
            approval_handler: Arc::new(AutoApproveHandler),
            event_sink: Arc::new(NoopEventSink),
            history,
        }
    }

    pub fn with_approval_handler<H: ApprovalHandler + 'static>(mut self, handler: H) -> Self {
        self.approval_handler = Arc::new(handler);
        self
    }

    pub fn with_event_sink<S: EventSink + 'static>(mut self, sink: S) -> Self {
        self.event_sink = Arc::new(sink);
        self
    }

    /// Discover workspace context (`AGENTS.md`, `CLAUDE.md`, `.hypercode.md`, shallow file tree)
    /// from `tool_ctx.workspace_root` and enrich the agent's system prompt.
    pub fn with_workspace_context(self) -> Self {
        let ws = WorkspaceContext::discover(&self.tool_ctx.workspace_root);
        self.with_discovered_context(&ws)
    }

    /// Enrich the agent's system prompt using an already-discovered `WorkspaceContext`.
    pub fn with_discovered_context(mut self, ws: &WorkspaceContext) -> Self {
        let enriched = ws.build_system_prompt(&self.config.system_prompt);
        self.config.system_prompt = enriched.clone();
        if let Some(first) = self.history.first_mut() {
            if first.role == hyperinfer_core::MessageRole::System {
                first.content = enriched;
                return self;
            }
        }
        self.history.insert(0, ChatMessage::system(enriched));
        self
    }

    /// Manually compact conversation history (e.g., when the user invokes `/compact`).
    pub async fn compact(&mut self) -> Result<bool, AgentError> {
        let tokens_before = estimate_messages_tokens(&self.history);
        let pruned = prune_old_tool_outputs(
            &mut self.history,
            self.config.compaction.keep_recent_messages,
            self.config.compaction.max_old_tool_output_chars,
        );
        let summarized = compact_history_with_llm(
            &mut self.history,
            self.config.compaction.keep_recent_messages,
            self.backend.as_ref(),
            &self.config.api_key_id,
            &self.config.model,
        )
        .await?;

        if pruned > 0 || summarized {
            let tokens_after = estimate_messages_tokens(&self.history);
            self.event_sink.on_event(&AgentEvent::ContextCompacted {
                tokens_before,
                tokens_after,
            });
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn maybe_auto_compact(&mut self) -> Result<(), AgentError> {
        let tokens_before = estimate_messages_tokens(&self.history);
        if tokens_before <= self.config.compaction.max_context_tokens {
            return Ok(());
        }

        // Tier 1: Prune oversized old tool outputs first
        let pruned = prune_old_tool_outputs(
            &mut self.history,
            self.config.compaction.keep_recent_messages,
            self.config.compaction.max_old_tool_output_chars,
        );

        // Tier 2: If still above threshold, summarize older turns via the LLM
        let after_tier1 = estimate_messages_tokens(&self.history);
        let mut summarized = false;
        if after_tier1 > self.config.compaction.max_context_tokens {
            summarized = compact_history_with_llm(
                &mut self.history,
                self.config.compaction.keep_recent_messages,
                self.backend.as_ref(),
                &self.config.api_key_id,
                &self.config.model,
            )
            .await?;
        }

        if pruned > 0 || summarized {
            let tokens_after = estimate_messages_tokens(&self.history);
            self.event_sink.on_event(&AgentEvent::ContextCompacted {
                tokens_before,
                tokens_after,
            });
        }
        Ok(())
    }

    /// Inspect the current conversation transcript.
    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    /// Estimate the current token count of the conversation history.
    pub fn estimated_tokens(&self) -> usize {
        estimate_messages_tokens(&self.history)
    }

    /// Append a message directly to the agent's conversation history.
    pub fn push_message(&mut self, message: ChatMessage) {
        self.history.push(message);
    }

    /// Clear conversation history (preserving the system prompt).
    pub fn reset_history(&mut self) {
        self.history.clear();
        if !self.config.system_prompt.trim().is_empty() {
            self.history
                .push(ChatMessage::system(self.config.system_prompt.clone()));
        }
    }

    /// Determine whether a tool call requires explicit approval under the current `ApprovalMode`
    /// and `CommandRunner` sandbox capabilities.
    fn check_needs_approval(&self, call: &ToolCall) -> Option<String> {
        let tool = self.registry.get(&call.name)?;
        let requested_bypass = call.name == "bash"
            && serde_json::from_str::<serde_json::Value>(&call.arguments)
                .ok()
                .and_then(|v| v.get("bypass_sandbox")?.as_bool())
                .unwrap_or(false);

        if requested_bypass {
            return Some("Command explicitly requested `bypass_sandbox: true`".to_string());
        }

        match self.config.approval_mode {
            ApprovalMode::AutoApprove => None,
            ApprovalMode::ApproveAll => Some(format!(
                "ApprovalMode::ApproveAll requires approval for '{}'",
                call.name
            )),
            ApprovalMode::ApproveMutations => {
                if tool.requires_approval() {
                    Some(format!("Tool '{}' requires mutation approval", call.name))
                } else {
                    None
                }
            }
            ApprovalMode::SmartSandbox => {
                if !tool.requires_approval() {
                    None
                } else if call.name == "bash" && self.tool_ctx.command_runner.is_sandboxed() {
                    // Running inside an isolated sandbox (e.g. OpenShell) without bypass_sandbox
                    None
                } else {
                    Some(format!(
                        "Tool '{}' modifies workspace or runs unsandboxed commands",
                        call.name
                    ))
                }
            }
        }
    }

    /// Run a complete ReAct turn for `user_prompt`:
    /// Think -> Call Tools -> Observe -> Repeat until final response or `max_steps`.
    pub async fn run_turn(
        &mut self,
        user_prompt: impl Into<String>,
    ) -> Result<TurnOutcome, AgentError> {
        let prompt = user_prompt.into();
        self.event_sink.on_event(&AgentEvent::TurnStarted {
            user_prompt: prompt.clone(),
        });
        self.history.push(ChatMessage::user(prompt));

        let tool_defs = {
            let defs = self.registry.definitions();
            if defs.is_empty() {
                None
            } else {
                Some(defs)
            }
        };

        let mut total_usage = Usage::default();
        let mut tool_calls_executed = 0usize;

        for step in 1..=self.config.max_steps {
            self.maybe_auto_compact().await?;
            self.event_sink.on_event(&AgentEvent::StepStarted { step });

            let request = ChatRequest {
                model: self.config.model.clone(),
                messages: self.history.clone(),
                temperature: self.config.temperature,
                max_tokens: self.config.max_tokens,
                stream: None,
                stop: None,
                tools: tool_defs.clone(),
            };

            let response = self.backend.chat(&self.config.api_key_id, request).await?;
            total_usage.input_tokens += response.usage.input_tokens;
            total_usage.output_tokens += response.usage.output_tokens;

            let choice = response
                .choices
                .into_iter()
                .next()
                .ok_or(AgentError::EmptyResponse)?;

            let assistant_msg = choice.message;
            let tool_calls = assistant_msg.tool_calls.clone().unwrap_or_default();

            self.event_sink.on_event(&AgentEvent::AssistantResponse {
                content: assistant_msg.content.clone(),
                tool_calls: tool_calls.clone(),
            });

            self.history.push(assistant_msg.clone());

            // Base case: the model did not request any tool calls -> turn is complete!
            if tool_calls.is_empty() {
                self.event_sink.on_event(&AgentEvent::TurnCompleted {
                    final_response: assistant_msg.content.clone(),
                    steps_taken: step,
                    total_usage: total_usage.clone(),
                });
                return Ok(TurnOutcome {
                    final_response: assistant_msg.content,
                    steps_taken: step,
                    tool_calls_executed,
                    total_usage,
                });
            }

            // Recursive step: execute each requested tool call and append observations.
            for tc in tool_calls {
                let output = if let Some(reason) = self.check_needs_approval(&tc) {
                    let approved = self.approval_handler.approve(&tc, &reason).await;
                    self.event_sink.on_event(&AgentEvent::ToolApprovalChecked {
                        tool_call: tc.clone(),
                        reason: reason.clone(),
                        approved,
                    });
                    if approved {
                        tool_calls_executed += 1;
                        self.registry.execute(&tc, &self.tool_ctx).await
                    } else {
                        ToolOutput::error(format!(
                            "Tool execution denied by user approval policy ({}).",
                            reason
                        ))
                    }
                } else {
                    tool_calls_executed += 1;
                    self.registry.execute(&tc, &self.tool_ctx).await
                };

                self.event_sink.on_event(&AgentEvent::ToolExecuted {
                    tool_call: tc.clone(),
                    output: output.clone(),
                });

                self.history.push(ChatMessage::tool_result(
                    tc.id.clone(),
                    tc.name.clone(),
                    output.content,
                ));
            }
        }

        Err(AgentError::MaxStepsExceeded {
            max_steps: self.config.max_steps,
        })
    }
}
