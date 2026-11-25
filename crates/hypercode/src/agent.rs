//! The HyperCode Agent Loop (ReAct Engine) and Permission/Sandbox Gate.

use crate::context::{
    compact_history_with_llm, estimate_messages_tokens, prune_old_tool_outputs, CompactionConfig,
    WorkspaceContext,
};
use crate::tool::{ToolContext, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use futures::future::join_all;
use futures::stream::once;
use futures::{Stream, StreamExt};
use hyperinfer_client::HyperInferClient;
use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, HyperInferError, ToolCall, Usage,
};
use serde::{Deserialize, Serialize};
use std::pin::Pin;
use std::sync::Arc;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

/// Errors that can terminate an agent turn.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("LLM provider error: {0}")]
    Llm(#[from] HyperInferError),

    #[error("Agent exceeded maximum step limit ({max_steps}) without completing turn")]
    MaxStepsExceeded { max_steps: usize },

    #[error("LLM returned an empty choices list")]
    EmptyResponse,

    #[error("Turn was cancelled")]
    Cancelled,
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
    /// Chat-only mode: no tools are sent to the LLM; the agent responds with text only.
    ChatOnly,
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
    /// Streaming delta of assistant text (emitted per chunk during streaming).
    AssistantDelta {
        text: String,
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

/// A stream of chat chunks returned by a streaming LLM call.
pub type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send>>;

/// Abstracts the LLM client so `Agent` works seamlessly with `HyperInferClient`
/// as well as deterministic mock backends in tests.
#[async_trait]
pub trait ChatBackend: Send + Sync {
    async fn chat(&self, key: &str, request: ChatRequest) -> Result<ChatResponse, HyperInferError>;

    /// Stream token chunks for a chat request. Default implementation wraps
    /// `chat()` in a single-chunk stream for backends that don't support streaming.
    async fn chat_stream(
        &self,
        key: &str,
        request: ChatRequest,
    ) -> Result<ChatStream, HyperInferError> {
        let response = self.chat(key, request).await?;
        let first_choice = response.choices.first().cloned();
        let chunk = ChatChunk {
            id: response.id,
            model: response.model,
            delta: first_choice
                .as_ref()
                .map(|c| c.message.content.clone())
                .unwrap_or_default(),
            finish_reason: first_choice.as_ref().and_then(|c| c.finish_reason.clone()),
            usage: Some(response.usage),
            tool_calls: first_choice
                .as_ref()
                .and_then(|c| c.message.tool_calls.clone())
                .filter(|tcs| !tcs.is_empty()),
        };
        Ok(Box::pin(once(async move { Ok(chunk) })))
    }
}

#[async_trait]
impl ChatBackend for HyperInferClient {
    async fn chat(&self, key: &str, request: ChatRequest) -> Result<ChatResponse, HyperInferError> {
        HyperInferClient::chat(self, key, request).await
    }

    async fn chat_stream(
        &self,
        key: &str,
        request: ChatRequest,
    ) -> Result<ChatStream, HyperInferError> {
        HyperInferClient::chat_stream(self, key, request).await
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
                "Prefer `edit_file` over `write_file` when modifying existing files.\n",
                "Batch independent `read_file`, `grep`, `glob`, and `edit_file` calls in parallel ",
                "within the same turn instead of issuing them one at a time.\n",
                "Always run relevant build, test, and lint checks after your edits and fix any ",
                "reported errors before finishing."
            )
            .to_string(),
            max_steps: 50,
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

    pub fn set_approval_handler<H: ApprovalHandler + 'static>(&mut self, handler: H) {
        self.approval_handler = Arc::new(handler);
    }

    pub fn set_event_sink<S: EventSink + 'static>(&mut self, sink: S) {
        self.event_sink = Arc::new(sink);
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
            ApprovalMode::ChatOnly => {
                Some("ApprovalMode::ChatOnly disables tool execution".to_string())
            }
        }
    }

    /// Run a complete ReAct turn for `user_prompt`:
    /// Think -> Call Tools -> Observe -> Repeat until final response or `max_steps`.
    /// Run a single agent turn with a cancellation token.
    pub async fn run_turn_with_cancel(
        &mut self,
        user_prompt: impl Into<String>,
        cancel: CancellationToken,
    ) -> Result<TurnOutcome, AgentError> {
        let prompt = user_prompt.into();
        self.event_sink.on_event(&AgentEvent::TurnStarted {
            user_prompt: prompt.clone(),
        });
        self.history.push(ChatMessage::user(prompt));

        // Build tool context with the cancellation token for this turn
        let turn_tool_ctx = self.tool_ctx.clone().with_cancel_token(cancel.clone());

        let tool_defs = if self.config.approval_mode == ApprovalMode::ChatOnly {
            None
        } else {
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
            if cancel.is_cancelled() {
                return Err(AgentError::Cancelled);
            }

            self.maybe_auto_compact().await?;
            self.event_sink.on_event(&AgentEvent::StepStarted { step });

            let request = ChatRequest {
                model: self.config.model.clone(),
                messages: self.history.clone(),
                temperature: self.config.temperature,
                max_tokens: self.config.max_tokens,
                stream: Some(true),
                stop: None,
                tools: tool_defs.clone(),
                tool_choice: None,
                response_format: None,
            };

            // Stream the response, racing against cancellation
            let stream = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(AgentError::Cancelled),
                res = self.backend.chat_stream(&self.config.api_key_id, request) => res?,
            };

            let mut full_content = String::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut stream_usage = None;

            let mut stream = stream;
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        return Err(AgentError::Cancelled);
                    }
                    chunk_result = stream.next() => {
                        match chunk_result {
                            Some(Ok(chunk)) => {
                                if !chunk.delta.is_empty() {
                                    full_content.push_str(&chunk.delta);
                                    self.event_sink.on_event(&AgentEvent::AssistantDelta {
                                        text: chunk.delta.clone(),
                                    });
                                }
                                if let Some(tcs) = chunk.tool_calls {
                                    tool_calls.extend(tcs);
                                }
                                if chunk.usage.is_some() {
                                    stream_usage = chunk.usage;
                                }
                            }
                            Some(Err(e)) => return Err(AgentError::Llm(e)),
                            None => break,
                        }
                    }
                }
            }

            let usage = stream_usage.unwrap_or_default();
            total_usage.input_tokens += usage.input_tokens;
            total_usage.output_tokens += usage.output_tokens;

            let assistant_msg = if tool_calls.is_empty() {
                ChatMessage::assistant(full_content.clone())
            } else {
                ChatMessage::assistant_with_tools(full_content.clone(), tool_calls.clone())
            };

            self.event_sink.on_event(&AgentEvent::AssistantResponse {
                content: full_content.clone(),
                tool_calls: tool_calls.clone(),
            });

            self.history.push(assistant_msg.clone());

            // Base case: no tool calls or chat-only mode -> turn is complete
            if tool_calls.is_empty() || self.config.approval_mode == ApprovalMode::ChatOnly {
                self.event_sink.on_event(&AgentEvent::TurnCompleted {
                    final_response: full_content.clone(),
                    steps_taken: step,
                    total_usage: total_usage.clone(),
                });
                return Ok(TurnOutcome {
                    final_response: full_content,
                    steps_taken: step,
                    tool_calls_executed,
                    total_usage,
                });
            }

            // Execute tool calls: batch read-only ones concurrently, run mutating ones sequentially
            let mut i = 0;
            while i < tool_calls.len() {
                if cancel.is_cancelled() {
                    return Err(AgentError::Cancelled);
                }

                // Find the start of a contiguous run of read-only, no-approval tool calls
                let tc = &tool_calls[i];
                let needs_approval = self.check_needs_approval(tc).is_some();
                let is_read_only = self
                    .registry
                    .get(&tc.name)
                    .map(|t| !t.requires_approval())
                    .unwrap_or(false);

                if !needs_approval && is_read_only {
                    // Find the end of this contiguous run
                    let mut j = i + 1;
                    while j < tool_calls.len() {
                        let tc_j = &tool_calls[j];
                        let needs_approval_j = self.check_needs_approval(tc_j).is_some();
                        let is_read_only_j = self
                            .registry
                            .get(&tc_j.name)
                            .map(|t| !t.requires_approval())
                            .unwrap_or(false);
                        if needs_approval_j || !is_read_only_j {
                            break;
                        }
                        j += 1;
                    }

                    // Execute the contiguous run concurrently while preserving original order
                    let batch = &tool_calls[i..j];
                    let registry = &self.registry;
                    let futures = batch.iter().map(|tc| {
                        let ctx = turn_tool_ctx.clone();
                        async move {
                            let output = registry.execute(tc, &ctx).await;
                            (tc.clone(), output)
                        }
                    });
                    let results = tokio::select! {
                        biased;
                        _ = cancel.cancelled() => return Err(AgentError::Cancelled),
                        res = join_all(futures) => res,
                    };

                    for (tc, output) in results {
                        tool_calls_executed += 1;
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

                    i = j;
                } else {
                    // Execute this single tool call sequentially (needs approval or is mutating)
                    let output = if let Some(reason) = self.check_needs_approval(tc) {
                        let approved = tokio::select! {
                            biased;
                            _ = cancel.cancelled() => return Err(AgentError::Cancelled),
                            res = self.approval_handler.approve(tc, &reason) => res,
                        };
                        self.event_sink.on_event(&AgentEvent::ToolApprovalChecked {
                            tool_call: tc.clone(),
                            reason: reason.clone(),
                            approved,
                        });
                        if !approved {
                            ToolOutput::error(format!(
                                "Tool execution denied by user approval policy ({}).",
                                reason
                            ))
                        } else {
                            tool_calls_executed += 1;
                            self.registry.execute(tc, &turn_tool_ctx).await
                        }
                    } else {
                        tool_calls_executed += 1;
                        self.registry.execute(tc, &turn_tool_ctx).await
                    };

                    if cancel.is_cancelled() {
                        return Err(AgentError::Cancelled);
                    }

                    self.event_sink.on_event(&AgentEvent::ToolExecuted {
                        tool_call: tc.clone(),
                        output: output.clone(),
                    });
                    self.history.push(ChatMessage::tool_result(
                        tc.id.clone(),
                        tc.name.clone(),
                        output.content,
                    ));

                    i += 1;
                }
            }
        }

        Err(AgentError::MaxStepsExceeded {
            max_steps: self.config.max_steps,
        })
    }

    /// Run a single agent turn (non-cancellable wrapper).
    pub async fn run_turn(
        &mut self,
        user_prompt: impl Into<String>,
    ) -> Result<TurnOutcome, AgentError> {
        self.run_turn_with_cancel(user_prompt, CancellationToken::new())
            .await
    }
}
