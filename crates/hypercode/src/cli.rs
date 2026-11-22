//! Terminal UI, CLI argument parsing, interactive approval prompt, and REPL loop for `hypercode`.

use crate::a2a::A2aServer;
use crate::agent::{Agent, AgentConfig, AgentEvent, ApprovalHandler, ApprovalMode, EventSink};
use crate::context::WorkspaceContext;
use crate::sandbox::{HostRunner, OpenShellRunner};
use crate::tool::{ToolContext, ToolRegistry};
use async_trait::async_trait;
use hyperinfer_client::HyperInferClient;
use hyperinfer_core::{Config, Provider, ToolCall};
use hyperinfer_providers::openai::OpenAiProvider;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;

/// Parsed command-line options for the `hypercode` binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOptions {
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub workspace: PathBuf,
    pub approval_mode: ApprovalMode,
    pub openshell_sandbox: Option<String>,
    pub local_url: Option<String>,
    pub max_steps: usize,
    pub a2a_port: Option<u16>,
    pub a2a_stdio: bool,
    pub show_help: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            model: None,
            prompt: None,
            workspace: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            approval_mode: ApprovalMode::SmartSandbox,
            openshell_sandbox: std::env::var("HYPERCODE_OPENSHELL_SANDBOX").ok(),
            local_url: std::env::var("HYPERCODE_LOCAL_URL").ok(),
            max_steps: 25,
            a2a_port: None,
            a2a_stdio: false,
            show_help: false,
        }
    }
}

impl CliOptions {
    /// Parse CLI arguments from an iterator of strings (excluding `argv[0]`).
    pub fn parse_from<I, S>(args: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut opts = Self::default();
        let mut iter = args.into_iter().map(Into::into);

        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "-h" | "--help" => {
                    opts.show_help = true;
                }
                "-m" | "--model" => {
                    opts.model = Some(
                        iter.next()
                            .ok_or_else(|| "Missing value for --model".to_string())?,
                    );
                }
                "-p" | "--prompt" => {
                    opts.prompt = Some(
                        iter.next()
                            .ok_or_else(|| "Missing value for --prompt".to_string())?,
                    );
                }
                "-w" | "--workspace" => {
                    let path = iter
                        .next()
                        .ok_or_else(|| "Missing value for --workspace".to_string())?;
                    opts.workspace = PathBuf::from(path);
                }
                "--approval" => {
                    let val = iter
                        .next()
                        .ok_or_else(|| "Missing value for --approval".to_string())?;
                    opts.approval_mode = match val.as_str() {
                        "auto" => ApprovalMode::AutoApprove,
                        "smart" => ApprovalMode::SmartSandbox,
                        "mutations" => ApprovalMode::ApproveMutations,
                        "all" => ApprovalMode::ApproveAll,
                        other => {
                            let msg = format!("Unknown --approval mode '{}'", other);
                            return Err(msg);
                        }
                    };
                }
                "--openshell" => {
                    opts.openshell_sandbox = Some(
                        iter.next()
                            .ok_or_else(|| "Missing value for --openshell".to_string())?,
                    );
                }
                "--local-url" => {
                    opts.local_url = Some(
                        iter.next()
                            .ok_or_else(|| "Missing value for --local-url".to_string())?,
                    );
                }
                "--max-steps" => {
                    let raw = iter
                        .next()
                        .ok_or_else(|| "Missing value for --max-steps".to_string())?;
                    opts.max_steps = raw
                        .parse::<usize>()
                        .map_err(|_| format!("Invalid --max-steps value: '{}'", raw))?;
                }
                "--a2a-port" => {
                    let raw = iter
                        .next()
                        .ok_or_else(|| "Missing value for --a2a-port".to_string())?;
                    opts.a2a_port = Some(
                        raw.parse::<u16>()
                            .map_err(|_| format!("Invalid --a2a-port value: '{}'", raw))?,
                    );
                }
                "--a2a-stdio" => {
                    opts.a2a_stdio = true;
                }
                other if other.starts_with('-') => {
                    return Err(format!("Unknown flag: '{}'. Use --help for usage.", other));
                }
                positional => {
                    // Treat trailing positional argument as single-shot prompt if `-p` wasn't set
                    if opts.prompt.is_none() {
                        opts.prompt = Some(positional.to_string());
                    } else {
                        return Err(format!("Unexpected positional argument: '{}'", positional));
                    }
                }
            }
        }

        Ok(opts)
    }

    /// Return the CLI help text.
    pub fn help_text() -> &'static str {
        "\
HyperCode — Provider-agnostic coding agent harness built on HyperInfer

USAGE:
    hypercode [OPTIONS] [PROMPT]

OPTIONS:
    -m, --model <MODEL>          Model name (e.g., gpt-4o, claude-3-5-sonnet-20241022,
                                 gemini-2.5-pro, antigravity/gemini-2.5-pro, qwen2.5-coder)
    -p, --prompt <PROMPT>        Run a single-shot task and exit
    -w, --workspace <PATH>       Workspace root directory (default: current directory)
        --approval <MODE>        Approval mode: smart (default), mutations, all, auto
        --openshell <NAME>       Run bash commands inside an NVIDIA OpenShell sandbox
        --local-url <URL>        OpenAI-compatible local endpoint (e.g. http://localhost:11434)
        --max-steps <N>          Max ReAct tool-calling iterations per turn (default: 25)
        --a2a-port <PORT>        Start an A2A HTTP/SSE JSON-RPC server on 127.0.0.1:<PORT>
        --a2a-stdio              Start an A2A JSON-RPC 2.0 server over stdin/stdout
    -h, --help                   Show this help message

ENVIRONMENT VARIABLES:
    OPENAI_API_KEY               API key for OpenAI models (gpt-*)
    ANTHROPIC_API_KEY            API key for Anthropic models (claude-*)
    GEMINI_API_KEY               API key for Google Gemini models (gemini-*)
    ANTIGRAVITY_API_KEY          API key for Antigravity models (antigravity/*)
    HYPERCODE_LOCAL_URL          Base URL for local Ollama / vLLM / LM Studio server
    HYPERCODE_OPENSHELL_SANDBOX  Default OpenShell sandbox name

REPL COMMANDS:
    /compact                     Summarize older conversation turns to free context tokens
    /tokens                      Show current message count, estimated tokens, and config
    /reset                       Clear conversation history (keeps system prompt)
    /help                        Show available REPL slash commands
    /quit, /exit                 Exit HyperCode"
    }
}

/// Interactive REPL slash commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplCommand {
    Help,
    Compact,
    Tokens,
    Reset,
    Quit,
}

impl ReplCommand {
    /// Parse a user input line if it starts with `/`.
    pub fn parse(input: &str) -> Option<Result<Self, String>> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }
        let cmd = trimmed.split_whitespace().next().unwrap_or("");
        match cmd {
            "/help" | "/h" | "/?" => Some(Ok(Self::Help)),
            "/compact" => Some(Ok(Self::Compact)),
            "/tokens" | "/status" => Some(Ok(Self::Tokens)),
            "/reset" | "/clear" => Some(Ok(Self::Reset)),
            "/quit" | "/exit" | "/q" => Some(Ok(Self::Quit)),
            other => Some(Err(format!(
                "Unknown slash command '{}'. Type /help for available commands.",
                other
            ))),
        }
    }
}

const MAX_DIFF_PREVIEW_LINES: usize = 30;

/// Format a human-readable preview of a tool call (including a unified diff for `edit_file`).
pub fn format_tool_call_preview(call: &ToolCall) -> String {
    let parsed: serde_json::Value = match serde_json::from_str(&call.arguments) {
        Ok(v) => v,
        Err(_) => return format!("{}({})", call.name, call.arguments),
    };

    match call.name.as_str() {
        "edit_file" => {
            let path = parsed
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            let target = parsed.get("target").and_then(|v| v.as_str()).unwrap_or("");
            let replacement = parsed
                .get("replacement")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let mut out = format!("edit_file `{}`:\n", path);
            let target_lines: Vec<&str> = target.lines().collect();
            for line in target_lines.iter().take(MAX_DIFF_PREVIEW_LINES) {
                out.push_str(&format!("  \x1b[31m- {}\x1b[0m\n", line));
            }
            if target_lines.len() > MAX_DIFF_PREVIEW_LINES {
                out.push_str(&format!(
                    "  \x1b[90m... (+{} more removed lines)\x1b[0m\n",
                    target_lines.len() - MAX_DIFF_PREVIEW_LINES
                ));
            }
            let replacement_lines: Vec<&str> = replacement.lines().collect();
            for line in replacement_lines.iter().take(MAX_DIFF_PREVIEW_LINES) {
                out.push_str(&format!("  \x1b[32m+ {}\x1b[0m\n", line));
            }
            if replacement_lines.len() > MAX_DIFF_PREVIEW_LINES {
                out.push_str(&format!(
                    "  \x1b[90m... (+{} more added lines)\x1b[0m\n",
                    replacement_lines.len() - MAX_DIFF_PREVIEW_LINES
                ));
            }
            out.trim_end().to_string()
        }
        "write_file" => {
            let path = parsed
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            let content = parsed.get("content").and_then(|v| v.as_str()).unwrap_or("");
            format!(
                "write_file `{}` ({} lines, {} bytes)",
                path,
                content.lines().count(),
                content.len()
            )
        }
        "bash" => {
            let cmd = parsed
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("<empty>");
            let bypass = parsed
                .get("bypass_sandbox")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if bypass {
                format!("bash [UNSANDBOXED]: `{}`", cmd)
            } else {
                format!("bash: `{}`", cmd)
            }
        }
        _ => format!("{}({})", call.name, call.arguments),
    }
}

/// Terminal `EventSink` that prints live agent progress to stdout.
pub struct TerminalEventSink;

impl EventSink for TerminalEventSink {
    fn on_event(&self, event: &AgentEvent) {
        match event {
            AgentEvent::TurnStarted { .. } => {}
            AgentEvent::StepStarted { step } => {
                println!("\x1b[90m── Step {} ──\x1b[0m", step);
            }
            AgentEvent::AssistantResponse {
                content,
                tool_calls,
            } => {
                if !content.trim().is_empty() && !tool_calls.is_empty() {
                    println!("\x1b[36m●\x1b[0m {}", content);
                }
                for call in tool_calls {
                    println!(
                        "\x1b[33m⚙ Tool Call:\x1b[0m {}",
                        format_tool_call_preview(call)
                    );
                }
            }
            AgentEvent::ToolApprovalChecked {
                tool_call,
                reason,
                approved,
            } => {
                if *approved {
                    println!(
                        "  \x1b[32m✓ Approved\x1b[0m `{}` ({})",
                        tool_call.name, reason
                    );
                } else {
                    println!(
                        "  \x1b[31m✗ Denied\x1b[0m `{}` ({})",
                        tool_call.name, reason
                    );
                }
            }
            AgentEvent::ToolExecuted { tool_call, output } => {
                let first_line = output.content.lines().next().unwrap_or("");
                let total_lines = output.content.lines().count();
                if output.is_error {
                    println!(
                        "  \x1b[31m✗ {} failed:\x1b[0m {}",
                        tool_call.name, first_line
                    );
                } else if total_lines > 1 {
                    println!(
                        "  \x1b[32m✓ {}\x1b[0m → {} (+{} more lines)",
                        tool_call.name,
                        first_line,
                        total_lines - 1
                    );
                } else {
                    println!("  \x1b[32m✓ {}\x1b[0m → {}", tool_call.name, first_line);
                }
            }
            AgentEvent::ContextCompacted {
                tokens_before,
                tokens_after,
            } => {
                println!(
                    "\x1b[35m⚡ Context compacted:\x1b[0m ~{} tokens → ~{} tokens",
                    tokens_before, tokens_after
                );
            }
            AgentEvent::TurnCompleted {
                steps_taken,
                total_usage,
                ..
            } => {
                println!(
                    "\x1b[90m[Turn finished: {} step(s), {} in / {} out tokens]\x1b[0m",
                    steps_taken, total_usage.input_tokens, total_usage.output_tokens
                );
            }
        }
    }
}

/// Interactive terminal `ApprovalHandler` that prompts the user with `[y/N]` on stdin.
pub struct TerminalApprovalHandler;

#[async_trait]
impl ApprovalHandler for TerminalApprovalHandler {
    async fn approve(&self, call: &ToolCall, reason: &str) -> bool {
        println!("\n\x1b[33;1m⚠ Approval Required\x1b[0m ({})", reason);
        println!("  {}", format_tool_call_preview(call));
        print!("\x1b[1mAllow this tool call? [y/N]: \x1b[0m");
        let _ = io::stdout().flush();

        tokio::task::spawn_blocking(|| {
            let stdin = io::stdin();
            let mut line = String::new();
            if stdin.lock().read_line(&mut line).is_ok() {
                matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
            } else {
                false
            }
        })
        .await
        .unwrap_or(false)
    }
}

/// Build a standalone `HyperInferClient` and select the default model from environment variables
/// and `CliOptions`.
pub async fn build_client_and_model(
    opts: &CliOptions,
) -> Result<(Arc<HyperInferClient>, String), String> {
    let mut api_keys = HashMap::new();
    let mut detected_model: Option<String> = None;
    let mut default_provider: Option<Provider> = None;

    if let Ok(key) = std::env::var("ANTIGRAVITY_API_KEY") {
        if !key.is_empty() {
            api_keys.insert("antigravity".to_string(), key);
            detected_model.get_or_insert_with(|| "antigravity/gemini-2.5-pro".to_string());
            default_provider.get_or_insert(Provider::Antigravity);
        }
    }
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            api_keys.insert("anthropic".to_string(), key);
            detected_model.get_or_insert_with(|| "claude-3-5-sonnet-20241022".to_string());
            default_provider.get_or_insert(Provider::Anthropic);
        }
    }
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        if !key.is_empty() {
            api_keys.insert("openai".to_string(), key);
            detected_model.get_or_insert_with(|| "gpt-4o".to_string());
            default_provider.get_or_insert(Provider::OpenAI);
        }
    }
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        if !key.is_empty() {
            api_keys.insert("gemini".to_string(), key);
            detected_model.get_or_insert_with(|| "gemini-2.5-pro".to_string());
            default_provider.get_or_insert(Provider::Gemini);
        }
    }

    if opts.local_url.is_some() {
        let local_key =
            std::env::var("HYPERCODE_LOCAL_API_KEY").unwrap_or_else(|_| "local".to_string());
        api_keys.insert("other".to_string(), local_key);
        detected_model = Some("qwen2.5-coder:latest".to_string());
        default_provider = Some(Provider::Other);
    }

    if api_keys.is_empty() {
        return Err(
            "No LLM provider configured. Set one of OPENAI_API_KEY, ANTHROPIC_API_KEY, \
             GEMINI_API_KEY, ANTIGRAVITY_API_KEY, or pass --local-url <URL>."
                .to_string(),
        );
    }

    let model = opts
        .model
        .clone()
        .or(detected_model)
        .unwrap_or_else(|| "gpt-4o".to_string());

    let config = Config {
        api_keys,
        routing_rules: Vec::new(),
        quotas: HashMap::new(),
        model_aliases: HashMap::new(),
        default_provider,
    };

    let client = HyperInferClient::new_local(config)
        .await
        .map_err(|e| format!("Failed to initialize HyperInferClient: {}", e))?;

    if let Some(ref local_url) = opts.local_url {
        let local_provider = OpenAiProvider::with_name_and_base_url("other", local_url)
            .map_err(|e| format!("Failed to configure local provider '{}': {}", local_url, e))?;
        client.register_provider(local_provider).await;
    }

    Ok((Arc::new(client), model))
}

/// Entrypoint for running the `hypercode` CLI (single-shot or interactive REPL).
pub async fn run_cli(opts: CliOptions) -> Result<(), String> {
    if opts.show_help {
        println!("{}", CliOptions::help_text());
        return Ok(());
    }

    let (client, model) = build_client_and_model(&opts).await?;
    let ws_context = WorkspaceContext::discover(&opts.workspace);

    let mut tool_ctx = ToolContext::new(&ws_context.workspace_root);
    let sandbox_label = if let Some(ref sb_name) = opts.openshell_sandbox {
        tool_ctx = tool_ctx.with_command_runner(OpenShellRunner::new(sb_name.clone()));
        format!("NVIDIA OpenShell ({})", sb_name)
    } else {
        tool_ctx = tool_ctx.with_command_runner(HostRunner);
        "Host (secret-env stripped)".to_string()
    };

    let agent_config = AgentConfig {
        model: model.clone(),
        max_steps: opts.max_steps,
        approval_mode: opts.approval_mode,
        ..Default::default()
    };

    if opts.a2a_stdio {
        let server = A2aServer::new(
            "stdio://hypercode",
            agent_config,
            client,
            ToolRegistry::with_default_coding_tools(),
            tool_ctx,
            ws_context,
        );
        return server.serve_stdio().await;
    }

    if let Some(port) = opts.a2a_port {
        let bind_addr = format!("127.0.0.1:{}", port);
        let url = format!("http://{}", bind_addr);
        println!(
            "HyperCode A2A server listening on {} (AgentCard: {}/.well-known/agent.json)",
            url, url
        );
        let server = A2aServer::new(
            url,
            agent_config,
            client,
            ToolRegistry::with_default_coding_tools(),
            tool_ctx,
            ws_context,
        );
        return server.serve_http(&bind_addr).await;
    }

    let mut agent = Agent::new(
        agent_config,
        client,
        ToolRegistry::with_default_coding_tools(),
        tool_ctx,
    )
    .with_discovered_context(&ws_context)
    .with_approval_handler(TerminalApprovalHandler)
    .with_event_sink(TerminalEventSink);

    // Single-shot mode (`hypercode -p "..."`)
    if let Some(prompt) = opts.prompt {
        let outcome = agent
            .run_turn(prompt)
            .await
            .map_err(|e| format!("Agent error: {}", e))?;
        println!("\n{}", outcome.final_response);
        return Ok(());
    }

    // Interactive REPL mode
    println!("\x1b[1;36m╭──────────────────────────────────────────────────────────╮\x1b[0m");
    println!("\x1b[1;36m│  HyperCode — Provider-Agnostic Coding Agent Harness      │\x1b[0m");
    println!("\x1b[1;36m╰──────────────────────────────────────────────────────────╯\x1b[0m");
    println!("  Model:        \x1b[1m{}\x1b[0m", model);
    println!(
        "  Workspace:    \x1b[1m{}\x1b[0m",
        ws_context.workspace_root.display()
    );
    println!("  Sandbox:      \x1b[1m{}\x1b[0m", sandbox_label);
    println!("  Approval:     \x1b[1m{:?}\x1b[0m", opts.approval_mode);
    if !ws_context.instruction_files.is_empty() {
        let names: Vec<&str> = ws_context
            .instruction_files
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        println!("  Instructions: \x1b[32m{}\x1b[0m", names.join(", "));
    }
    println!("  Type \x1b[1m/help\x1b[0m for commands or \x1b[1m/quit\x1b[0m to exit.\n");

    let stdin = io::stdin();
    loop {
        print!("\x1b[1;32mhypercode>\x1b[0m ");
        let _ = io::stdout().flush();

        let mut input = String::new();
        let bytes = stdin
            .lock()
            .read_line(&mut input)
            .map_err(|e| format!("Failed to read stdin: {}", e))?;
        if bytes == 0 {
            // EOF (Ctrl-D)
            println!();
            break;
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(cmd_res) = ReplCommand::parse(trimmed) {
            match cmd_res {
                Ok(ReplCommand::Help) => {
                    println!(
                        "Available commands:\n  \
                         /compact   Summarize older conversation turns\n  \
                         /tokens    Show message count and estimated token usage\n  \
                         /reset     Clear conversation history\n  \
                         /quit      Exit HyperCode"
                    );
                }
                Ok(ReplCommand::Compact) => match agent.compact().await {
                    Ok(true) => {
                        println!(
                            "Compaction complete (~{} tokens remaining).",
                            agent.estimated_tokens()
                        );
                    }
                    Ok(false) => {
                        println!("History is already compact; nothing to summarize.");
                    }
                    Err(e) => {
                        eprintln!("\x1b[31mCompaction failed:\x1b[0m {}", e);
                    }
                },
                Ok(ReplCommand::Tokens) => {
                    println!(
                        "Messages: {} | Estimated tokens: ~{} | Model: {} | Sandbox: {}",
                        agent.history().len(),
                        agent.estimated_tokens(),
                        model,
                        sandbox_label
                    );
                }
                Ok(ReplCommand::Reset) => {
                    agent.reset_history();
                    println!("Conversation history reset.");
                }
                Ok(ReplCommand::Quit) => {
                    break;
                }
                Err(msg) => {
                    eprintln!("\x1b[31m{}\x1b[0m", msg);
                }
            }
            continue;
        }

        match agent.run_turn(trimmed).await {
            Ok(outcome) => {
                if !outcome.final_response.trim().is_empty() {
                    println!(
                        "\n\x1b[1;36mAssistant:\x1b[0m\n{}\n",
                        outcome.final_response
                    );
                }
            }
            Err(e) => {
                eprintln!("\n\x1b[31;1mError:\x1b[0m {}\n", e);
            }
        }
    }

    Ok(())
}
