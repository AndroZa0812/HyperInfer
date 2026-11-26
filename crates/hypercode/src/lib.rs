//! HyperCode — Agentic coding harness built on HyperInfer.

pub mod a2a;
pub mod agent;
pub mod cli;
pub mod context;
pub mod output_filter;
pub mod sandbox;
pub mod tool;
pub mod tools;

pub use a2a::{
    A2aServer, A2aStreamEvent, AgentCapabilities, AgentCard, AgentSkill, Artifact, JsonRpcError,
    JsonRpcRequest, JsonRpcResponse, Message as A2aMessage, Part as A2aPart, Task as A2aTask,
    TaskState, TaskStatus,
};
pub use agent::{
    Agent, AgentConfig, AgentError, AgentEvent, ApprovalHandler, ApprovalMode, AutoApproveHandler,
    ChatBackend, ChatStream, EventSink, NoopEventSink, RejectAllHandler, TurnOutcome,
};
pub use cli::{
    build_client_and_model, format_tool_call_preview, run_cli, CliOptions, ReplCommand,
    TerminalApprovalHandler, TerminalEventSink,
};
pub use context::{
    compact_history_with_llm, compact_history_with_llm_usage, deduplicate_read_file_outputs,
    estimate_messages_tokens, estimate_tokens, find_safe_compaction_split, model_context_profile,
    model_context_profile_from_catalog, prune_old_tool_outputs, refresh_model_catalog,
    set_model_catalog, CompactionConfig, ModelContextProfile, ModelUsage, TokenAccounting,
    WorkspaceContext, INSTRUCTION_FILES,
};
pub use output_filter::{
    FilterOutcome, FilterStats, OutputFilter, OutputFilterConfig, SessionFilterSnapshot,
    SessionFilterStats,
};
pub use sandbox::{
    validate_sandbox_name, CommandExecution, CommandRunner, HostRunner, OpenShellRunner,
    SandboxPolicy, SENSITIVE_ENV_VARS,
};
pub use tool::{Tool, ToolContext, ToolOutput, ToolRegistry};
pub use tools::{
    register_default_tools, BashTool, EditFileTool, GrepTool, ListFilesTool, ReadFileTool,
    WriteFileTool,
};

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use hyperinfer_core::{
        ChatMessage, ChatRequest, ChatResponse, Choice, HyperInferError, ToolCall, Usage,
    };
    use std::collections::VecDeque;
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    fn make_call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: format!("call_{}", name),
            name: name.to_string(),
            arguments: args.to_string(),
        }
    }

    struct ScriptedBackend {
        responses: Mutex<VecDeque<ChatMessage>>,
        seen_requests: Mutex<Vec<ChatRequest>>,
    }

    impl ScriptedBackend {
        fn new(responses: Vec<ChatMessage>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                seen_requests: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl ChatBackend for ScriptedBackend {
        async fn chat(
            &self,
            _key: &str,
            request: ChatRequest,
        ) -> Result<ChatResponse, HyperInferError> {
            self.seen_requests.lock().unwrap().push(request.clone());
            let next_msg = self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_else(|| ChatMessage::assistant("done"));
            Ok(ChatResponse {
                id: "mock-resp".to_string(),
                model: request.model,
                choices: vec![Choice {
                    index: 0,
                    message: next_msg,
                    finish_reason: Some("stop".to_string()),
                }],
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    ..Default::default()
                },
            })
        }
    }

    #[test]
    fn test_registry_default_definitions_sorted_and_approval_flags() {
        let registry = ToolRegistry::with_default_coding_tools();
        let defs = registry.definitions();
        let names: Vec<&str> = defs.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "bash",
                "edit_file",
                "grep",
                "list_files",
                "read_file",
                "write_file"
            ]
        );

        assert!(!registry.get("read_file").unwrap().requires_approval());
        assert!(!registry.get("list_files").unwrap().requires_approval());
        assert!(!registry.get("grep").unwrap().requires_approval());
        assert!(registry.get("write_file").unwrap().requires_approval());
        assert!(registry.get("edit_file").unwrap().requires_approval());
        assert!(registry.get("bash").unwrap().requires_approval());
    }

    #[test]
    fn test_tool_context_rejects_path_traversal_and_nested_symlink_escape() {
        let tmp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let ctx = ToolContext::new(tmp.path());

        assert!(ctx.resolve_path("src/main.rs").is_ok());
        assert!(ctx.resolve_path("../outside.txt").is_err());
        assert!(ctx.resolve_path("/etc/passwd").is_err());

        // Create a symlink inside the workspace pointing to `outside`
        #[cfg(unix)]
        {
            let link_path = tmp.path().join("symlink_out");
            std::os::unix::fs::symlink(outside.path(), &link_path).unwrap();
            // Even when nested subdirectories under the symlink do not exist yet,
            // `resolve_path` must canonicalize the deepest existing ancestor (`symlink_out`)
            // and reject the escape.
            assert!(ctx.resolve_path("symlink_out/pwned.txt").is_err());
            assert!(ctx
                .resolve_path("symlink_out/nonexistent_dir/deep/pwned.txt")
                .is_err());
        }
    }

    #[test]
    fn test_openshell_policy_yaml_escaping_and_sandbox_name_validation() {
        let policy = SandboxPolicy::restricted_for_workspace("/workspace/my \"quoted\" project")
            .with_allowed_domains(vec![
                "crates.io".to_string(),
                "github.com\"\n  injected: true".to_string(),
            ]);
        let yaml = policy.to_openshell_policy_yaml();
        assert!(yaml.contains("read_only:"));
        assert!(yaml.contains("- \"/workspace/my \\\"quoted\\\" project\""));
        assert!(yaml.contains("mode: \"egress_allowlist\""));
        assert!(yaml.contains("- \"crates.io\""));
        assert!(yaml.contains("- \"github.com\\\"\\n  injected: true\""));

        assert!(validate_sandbox_name("hypercode-dev_01").is_ok());
        assert!(validate_sandbox_name("").is_err());
        assert!(validate_sandbox_name("../escape").is_err());
        assert!(validate_sandbox_name("bad/name").is_err());

        let runner = OpenShellRunner::new("hypercode-dev");
        let args = runner.build_args("cargo test", Path::new("/tmp/policy.yaml"));
        assert_eq!(
            args,
            vec![
                "sandbox",
                "exec",
                "--name",
                "hypercode-dev",
                "--policy",
                "/tmp/policy.yaml",
                "--",
                "sh",
                "-c",
                "cargo test"
            ]
        );
        assert!(runner.is_sandboxed());
    }

    #[test]
    fn test_host_runner_strips_secret_env_vars() {
        let policy = SandboxPolicy::restricted_for_workspace("/tmp");
        let mut cmd = tokio::process::Command::new("sh");
        // Populate a custom `_API_KEY` var on the Command and verify `apply_env_policy`
        // strips all `SENSITIVE_ENV_VARS` without mutating process-global `std::env`.
        sandbox::apply_env_policy(&mut cmd, &policy);
        let envs: Vec<_> = cmd.as_std().get_envs().collect();
        for &secret in SENSITIVE_ENV_VARS {
            assert!(
                envs.iter()
                    .any(|(k, v)| *k == std::ffi::OsStr::new(secret) && v.is_none()),
                "expected {} to be stripped via env_remove",
                secret
            );
        }
    }

    #[tokio::test]
    async fn test_write_read_and_edit_file_workflow() {
        let tmp = TempDir::new().unwrap();
        let ctx = ToolContext::new(tmp.path());
        let registry = ToolRegistry::with_default_coding_tools();

        // 1. Write a new file in a nested subdirectory
        let write_out = registry
            .execute(
                &make_call(
                    "write_file",
                    serde_json::json!({
                        "path": "src/lib.rs",
                        "content": "pub fn add(a: i32, b: i32) -> i32 {\n    a - b\n}\n"
                    }),
                ),
                &ctx,
            )
            .await;
        assert!(!write_out.is_error, "{}", write_out.content);
        assert!(write_out.content.contains("src/lib.rs"));

        // 2. Read the file with line numbers
        let read_out = registry
            .execute(
                &make_call("read_file", serde_json::json!({ "path": "src/lib.rs" })),
                &ctx,
            )
            .await;
        assert!(!read_out.is_error);
        assert!(read_out
            .content
            .contains("   1: pub fn add(a: i32, b: i32) -> i32 {"));
        assert!(read_out.content.contains("   2:     a - b"));

        // 3. Edit the bug (`a - b` -> `a + b`) on line 2
        let edit_out = registry
            .execute(
                &make_call(
                    "edit_file",
                    serde_json::json!({
                        "path": "src/lib.rs",
                        "target": "    a - b",
                        "replacement": "    a + b"
                    }),
                ),
                &ctx,
            )
            .await;
        assert!(!edit_out.is_error, "{}", edit_out.content);
        assert!(
            edit_out.content.contains("at line 2"),
            "expected line 2 in '{}'",
            edit_out.content
        );

        // 4. Verify updated content
        let verify_out = registry
            .execute(
                &make_call(
                    "read_file",
                    serde_json::json!({
                        "path": "src/lib.rs",
                        "start_line": 2,
                        "end_line": 2
                    }),
                ),
                &ctx,
            )
            .await;
        assert!(!verify_out.is_error);
        assert!(verify_out.content.contains("   2:     a + b"));
    }

    #[tokio::test]
    async fn test_edit_file_rejects_missing_or_ambiguous_target() {
        let tmp = TempDir::new().unwrap();
        let ctx = ToolContext::new(tmp.path());
        let registry = ToolRegistry::with_default_coding_tools();

        registry
            .execute(
                &make_call(
                    "write_file",
                    serde_json::json!({
                        "path": "dup.txt",
                        "content": "hello\nworld\nhello\n"
                    }),
                ),
                &ctx,
            )
            .await;

        // Missing target
        let missing = registry
            .execute(
                &make_call(
                    "edit_file",
                    serde_json::json!({
                        "path": "dup.txt",
                        "target": "nonexistent",
                        "replacement": "x"
                    }),
                ),
                &ctx,
            )
            .await;
        assert!(missing.is_error);
        assert!(missing.content.contains("not found"));

        // Ambiguous target (matches twice)
        let ambiguous = registry
            .execute(
                &make_call(
                    "edit_file",
                    serde_json::json!({
                        "path": "dup.txt",
                        "target": "hello",
                        "replacement": "hi"
                    }),
                ),
                &ctx,
            )
            .await;
        assert!(ambiguous.is_error);
        assert!(ambiguous.content.contains("matched 2 locations"));
    }

    #[tokio::test]
    async fn test_list_files_and_grep() {
        let tmp = TempDir::new().unwrap();
        let ctx = ToolContext::new(tmp.path());
        let registry = ToolRegistry::with_default_coding_tools();

        // Create workspace files and an ignored .git file
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "secret_token").unwrap();

        registry
            .execute(
                &make_call(
                    "write_file",
                    serde_json::json!({
                        "path": "src/main.rs",
                        "content": "fn main() {\n    println!(\"HyperCode rocks\");\n}\n"
                    }),
                ),
                &ctx,
            )
            .await;

        let list_out = registry
            .execute(&make_call("list_files", serde_json::json!({})), &ctx)
            .await;
        assert!(!list_out.is_error);
        assert!(list_out.content.contains("src/"));
        assert!(list_out.content.contains("src/main.rs"));
        assert!(!list_out.content.contains(".git"));

        let grep_out = registry
            .execute(
                &make_call(
                    "grep",
                    serde_json::json!({ "pattern": "HyperCode\\s+rocks" }),
                ),
                &ctx,
            )
            .await;
        assert!(!grep_out.is_error);
        assert!(grep_out
            .content
            .contains("src/main.rs:2:     println!(\"HyperCode rocks\");"));
    }

    #[tokio::test]
    async fn test_bash_tool_execution_and_timeout() {
        let tmp = TempDir::new().unwrap();
        let ctx = ToolContext::new(tmp.path());
        let registry = ToolRegistry::with_default_coding_tools();

        let ok_out = registry
            .execute(
                &make_call(
                    "bash",
                    serde_json::json!({ "command": "echo hello_hypercode" }),
                ),
                &ctx,
            )
            .await;
        assert!(!ok_out.is_error);
        assert!(ok_out.content.contains("Exit code: 0"));
        assert!(ok_out.content.contains("hello_hypercode"));

        let fail_out = registry
            .execute(
                &make_call("bash", serde_json::json!({ "command": "exit 42" })),
                &ctx,
            )
            .await;
        assert!(fail_out.is_error);
        assert!(fail_out.content.contains("Exit code: 42"));

        let timeout_out = registry
            .execute(
                &make_call(
                    "bash",
                    serde_json::json!({ "command": "sleep 5", "timeout_secs": 1 }),
                ),
                &ctx,
            )
            .await;
        assert!(timeout_out.is_error);
        assert!(timeout_out.content.contains("timed out after 1s"));
    }

    #[tokio::test]
    async fn test_agent_multi_step_react_loop_fixes_bug() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("math.rs"),
            "pub fn multiply(x: i32, y: i32) -> i32 {\n    x + y\n}\n",
        )
        .unwrap();

        let backend = Arc::new(ScriptedBackend::new(vec![
            // Step 1: LLM decides to read `math.rs`
            ChatMessage::assistant_with_tools(
                "Let me inspect math.rs first.",
                vec![make_call(
                    "read_file",
                    serde_json::json!({ "path": "math.rs" }),
                )],
            ),
            // Step 2: After seeing file contents, LLM edits `x + y` -> `x * y`
            ChatMessage::assistant_with_tools(
                "I found the bug on line 2. Fixing it now.",
                vec![make_call(
                    "edit_file",
                    serde_json::json!({
                        "path": "math.rs",
                        "target": "    x + y",
                        "replacement": "    x * y"
                    }),
                )],
            ),
            // Step 3: LLM returns final summary
            ChatMessage::assistant("Fixed `multiply` in `math.rs` to use `x * y`."),
        ]));

        let mut agent = Agent::new(
            AgentConfig::default(),
            backend.clone(),
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(tmp.path()),
        );

        let outcome = agent
            .run_turn("Fix the bug in math.rs")
            .await
            .expect("agent turn should succeed");

        assert_eq!(outcome.steps_taken, 3);
        assert_eq!(outcome.tool_calls_executed, 2);
        assert_eq!(outcome.total_usage.input_tokens, 30);
        assert_eq!(outcome.total_usage.output_tokens, 15);
        assert!(outcome.final_response.contains("Fixed `multiply`"));

        let updated = std::fs::read_to_string(tmp.path().join("math.rs")).unwrap();
        assert!(updated.contains("x * y"));
    }

    #[tokio::test]
    async fn test_agent_approval_denial_feeds_error_to_model() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("notes.txt"), "keep me safe\n").unwrap();

        let backend = Arc::new(ScriptedBackend::new(vec![
            // Step 1: Read-only tool (`read_file`) -> auto-approved in SmartSandbox mode!
            ChatMessage::assistant_with_tools(
                "",
                vec![make_call(
                    "read_file",
                    serde_json::json!({ "path": "notes.txt" }),
                )],
            ),
            // Step 2: Mutating tool (`write_file`) -> rejected by RejectAllHandler!
            ChatMessage::assistant_with_tools(
                "",
                vec![make_call(
                    "write_file",
                    serde_json::json!({
                        "path": "notes.txt",
                        "content": "overwritten!"
                    }),
                )],
            ),
            // Step 3: Model sees denial message and responds gracefully
            ChatMessage::assistant(
                "Understood, I will not overwrite notes.txt without permission.",
            ),
        ]));

        let mut agent = Agent::new(
            AgentConfig {
                approval_mode: ApprovalMode::SmartSandbox,
                ..Default::default()
            },
            backend,
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(tmp.path()),
        )
        .with_approval_handler(RejectAllHandler);

        let outcome = agent
            .run_turn("Read notes.txt and overwrite it")
            .await
            .unwrap();

        assert_eq!(outcome.steps_taken, 3);
        // Only `read_file` was executed; `write_file` was blocked by approval gate
        assert_eq!(outcome.tool_calls_executed, 1);

        // File on disk was untouched
        let disk_content = std::fs::read_to_string(tmp.path().join("notes.txt")).unwrap();
        assert_eq!(disk_content, "keep me safe\n");
    }

    #[test]
    fn test_workspace_context_discovery_and_system_prompt() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("AGENTS.md"),
            "Always run `cargo test` before finishing.",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(tmp.path().join("src/lib.rs"), "pub fn demo() {}\n").unwrap();

        let ws = WorkspaceContext::discover(tmp.path());
        assert_eq!(ws.instruction_files.len(), 1);
        assert_eq!(ws.instruction_files[0].0, "AGENTS.md");
        assert!(ws.file_tree.contains(&"src/".to_string()));
        assert!(ws.file_tree.contains(&"src/lib.rs".to_string()));

        let prompt = ws.build_system_prompt("Base system prompt.");
        assert!(prompt.contains("Base system prompt."));
        assert!(prompt.contains("src/lib.rs"));
        assert!(prompt.contains("<project_instructions source=\"AGENTS.md\">"));
        assert!(prompt.contains("Always run `cargo test` before finishing."));
    }

    #[tokio::test]
    async fn test_two_tier_context_compaction_preserves_tool_call_pairs() {
        let tmp = TempDir::new().unwrap();
        let backend = Arc::new(ScriptedBackend::new(vec![ChatMessage::assistant(
            "Summary: User inspected big.txt and fixed config.toml.",
        )]));

        let mut agent = Agent::new(
            AgentConfig {
                compaction: CompactionConfig {
                    max_context_tokens: 100, // small threshold to test compaction
                    keep_recent_messages: 2,
                    max_old_tool_output_chars: 80,
                    compaction_ratio: 0.75,
                    auto_model_budget: false,
                },
                ..Default::default()
            },
            backend,
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(tmp.path()),
        );

        let mut hist = agent.history().to_vec();
        // Turn 1 (older turn with huge tool output)
        hist.push(ChatMessage::user("Read big.txt"));
        hist.push(ChatMessage::assistant_with_tools(
            "",
            vec![make_call(
                "read_file",
                serde_json::json!({ "path": "big.txt" }),
            )],
        ));
        hist.push(ChatMessage::tool_result(
            "call_read_file",
            "read_file",
            "A".repeat(2000),
        ));
        hist.push(ChatMessage::assistant("big.txt has 2000 A's."));

        // Turn 2 (recent turn that must be preserved verbatim)
        hist.push(ChatMessage::user("Now check status"));
        hist.push(ChatMessage::assistant("Everything looks clean."));

        // Tier 1 check: pruning old tool outputs shrinks the 2000-char tool result
        // to `<= max_chars` (120) and is idempotent on subsequent calls.
        let pruned = prune_old_tool_outputs(&mut hist, 2, 120);
        assert_eq!(pruned, 1);
        assert!(hist[3]
            .content
            .contains("chars pruned by context compactor"));
        assert!(
            hist[3].content.len() <= 120,
            "pruned len {} exceeded 120",
            hist[3].content.len()
        );
        assert_eq!(prune_old_tool_outputs(&mut hist, 2, 120), 0);
        // Even with a tiny `max_chars` (< PRUNE_MARKER_RESERVE), clamping prevents re-pruning.
        assert_eq!(prune_old_tool_outputs(&mut hist, 2, 10), 1);
        assert_eq!(prune_old_tool_outputs(&mut hist, 2, 10), 0);

        // Tier 2 check: full compaction via Agent::compact()
        for msg in hist.into_iter().skip(1) {
            agent.push_message(msg);
        }
        let tokens_before = agent.estimated_tokens();
        let did_compact = agent.compact().await.unwrap();
        assert!(did_compact);
        let tokens_after = agent.estimated_tokens();
        assert!(tokens_after < tokens_before);

        // History should now be:
        // [0] System
        // [1] User ("[Previous conversation history compacted]")
        // [2] Assistant ("<conversation_summary>...")
        // [3] User ("Now check status")
        // [4] Assistant ("Everything looks clean.")
        let compacted = agent.history();
        assert_eq!(compacted.len(), 5);
        assert!(compacted[2].content.contains("<conversation_summary>"));
        assert!(compacted[2]
            .content
            .contains("Summary: User inspected big.txt"));
        assert_eq!(compacted[3].content, "Now check status");
        assert_eq!(compacted[4].content, "Everything looks clean.");
    }

    #[test]
    fn test_cli_options_parsing_and_repl_commands() {
        let opts = CliOptions::parse_from([
            "-m",
            "antigravity/gemini-2.5-pro",
            "--approval",
            "mutations",
            "--openshell",
            "hc-sandbox",
            "--max-steps",
            "15",
            "-p",
            "Refactor src/lib.rs",
        ])
        .unwrap();

        assert_eq!(opts.model.as_deref(), Some("antigravity/gemini-2.5-pro"));
        assert_eq!(opts.approval_mode, ApprovalMode::ApproveMutations);
        assert_eq!(opts.openshell_sandbox.as_deref(), Some("hc-sandbox"));
        assert_eq!(opts.max_steps, 15);
        assert_eq!(opts.prompt.as_deref(), Some("Refactor src/lib.rs"));

        assert_eq!(
            ReplCommand::parse("/compact"),
            Some(Ok(ReplCommand::Compact))
        );
        assert_eq!(ReplCommand::parse("/tokens"), Some(Ok(ReplCommand::Tokens)));
        assert_eq!(ReplCommand::parse("/reset"), Some(Ok(ReplCommand::Reset)));
        assert_eq!(ReplCommand::parse("/quit"), Some(Ok(ReplCommand::Quit)));
        assert!(ReplCommand::parse("Fix the bug").is_none());
        assert!(ReplCommand::parse("/unknown").unwrap().is_err());
    }

    #[test]
    fn test_format_tool_call_preview_diff_and_bash() {
        let edit_call = make_call(
            "edit_file",
            serde_json::json!({
                "path": "src/main.rs",
                "target": "let x = 1;",
                "replacement": "let x = 2;"
            }),
        );
        let preview = format_tool_call_preview(&edit_call);
        assert!(preview.contains("edit_file `src/main.rs`:"));
        assert!(preview.contains("- let x = 1;"));
        assert!(preview.contains("+ let x = 2;"));

        let bash_call = make_call(
            "bash",
            serde_json::json!({
                "command": "git status",
                "bypass_sandbox": true
            }),
        );
        let bash_preview = format_tool_call_preview(&bash_call);
        assert_eq!(bash_preview, "bash [UNSANDBOXED]: `git status`");
    }

    #[tokio::test]
    async fn test_a2a_agent_card_and_input_required_approval_flow() {
        let tmp = TempDir::new().unwrap();
        let ws = WorkspaceContext::discover(tmp.path());
        let backend = Arc::new(ScriptedBackend::new(vec![
            // Step 1: Agent wants to write `hello.txt` (triggers InputRequired in SmartSandbox)
            ChatMessage::assistant_with_tools(
                "Writing hello.txt",
                vec![make_call(
                    "write_file",
                    serde_json::json!({
                        "path": "hello.txt",
                        "content": "hello from a2a\n"
                    }),
                )],
            ),
            // Step 2: After approval and execution, agent completes
            ChatMessage::assistant("Created hello.txt successfully."),
        ]));

        let server = A2aServer::new(
            "http://127.0.0.1:41241",
            AgentConfig {
                approval_mode: ApprovalMode::SmartSandbox,
                ..Default::default()
            },
            backend,
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(tmp.path()),
            ws,
        );

        // 1. Agent Card discovery via JSON-RPC
        let card_resp = server
            .handle_rpc(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: Some(serde_json::json!(1)),
                method: "agent/card".to_string(),
                params: serde_json::json!({}),
            })
            .await;
        assert!(card_resp.error.is_none());
        let card: AgentCard = serde_json::from_value(card_resp.result.unwrap()).unwrap();
        assert_eq!(card.name, "HyperCode");
        assert!(card.capabilities.streaming);

        // 2. Send task -> pauses in `InputRequired` for `write_file` approval
        let send_resp = server
            .handle_rpc(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: Some(serde_json::json!(2)),
                method: "tasks/send".to_string(),
                params: serde_json::json!({
                    "id": "task-1",
                    "sessionId": "sess-1",
                    "message": {
                        "role": "user",
                        "parts": [{ "type": "text", "text": "Create hello.txt" }]
                    }
                }),
            })
            .await;
        assert!(send_resp.error.is_none(), "{:?}", send_resp.error);
        let paused_task: A2aTask = serde_json::from_value(send_resp.result.unwrap()).unwrap();
        assert_eq!(paused_task.status.state, TaskState::InputRequired);

        // 3. Approve via `tasks/send` on the same `task-1` -> completes!
        let approve_resp = server
            .handle_rpc(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: Some(serde_json::json!(3)),
                method: "tasks/send".to_string(),
                params: serde_json::json!({
                    "id": "task-1",
                    "message": {
                        "role": "user",
                        "parts": [{ "type": "text", "text": "approve" }]
                    }
                }),
            })
            .await;
        assert!(approve_resp.error.is_none(), "{:?}", approve_resp.error);
        let done_task: A2aTask = serde_json::from_value(approve_resp.result.unwrap()).unwrap();
        assert_eq!(done_task.status.state, TaskState::Completed);
        assert!(!done_task.artifacts.is_empty());

        // Verify file was written on disk
        let written = std::fs::read_to_string(tmp.path().join("hello.txt")).unwrap();
        assert_eq!(written, "hello from a2a\n");

        // 4. `tasks/get` returns the completed task
        let get_resp = server
            .handle_rpc(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id: Some(serde_json::json!(4)),
                method: "tasks/get".to_string(),
                params: serde_json::json!({ "id": "task-1" }),
            })
            .await;
        assert!(get_resp.error.is_none());
        let fetched: A2aTask = serde_json::from_value(get_resp.result.unwrap()).unwrap();
        assert_eq!(fetched.status.state, TaskState::Completed);
    }

    #[test]
    fn test_cli_a2a_flags() {
        let opts_port = CliOptions::parse_from(["--a2a-port", "41241"]).unwrap();
        assert_eq!(opts_port.a2a_port, Some(41241));
        assert!(!opts_port.a2a_stdio);

        let opts_stdio = CliOptions::parse_from(["--a2a-stdio"]).unwrap();
        assert!(opts_stdio.a2a_stdio);
    }

    // --- Streaming & Cancellation Tests ---

    struct CollectingSink {
        events: Arc<Mutex<Vec<AgentEvent>>>,
    }

    impl EventSink for CollectingSink {
        fn on_event(&self, event: &AgentEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    /// Mock ChatBackend that streams multiple ChatChunk deltas.
    struct StreamingBackend {
        chunks: Vec<hyperinfer_core::ChatChunk>,
    }

    #[async_trait]
    impl ChatBackend for StreamingBackend {
        async fn chat(
            &self,
            _api_key_id: &str,
            _request: ChatRequest,
        ) -> Result<ChatResponse, HyperInferError> {
            Err(HyperInferError::Config(std::io::Error::other(
                "chat() not implemented",
            )))
        }

        async fn chat_stream(
            &self,
            _api_key_id: &str,
            _request: ChatRequest,
        ) -> Result<ChatStream, HyperInferError> {
            let items: Vec<Result<hyperinfer_core::ChatChunk, HyperInferError>> =
                self.chunks.clone().into_iter().map(Ok).collect();
            Ok(Box::pin(futures::stream::iter(items)))
        }
    }

    #[tokio::test]
    async fn test_streaming_emits_assistant_delta_events() {
        let chunks = vec![
            hyperinfer_core::ChatChunk {
                id: "stream-1".to_string(),
                model: "test".to_string(),
                delta: "Hello ".to_string(),
                finish_reason: None,
                usage: None,
                tool_calls: None,
            },
            hyperinfer_core::ChatChunk {
                id: "stream-2".to_string(),
                model: "test".to_string(),
                delta: "World".to_string(),
                finish_reason: None,
                usage: None,
                tool_calls: None,
            },
            hyperinfer_core::ChatChunk {
                id: "stream-3".to_string(),
                model: "test".to_string(),
                delta: "!".to_string(),
                finish_reason: Some("stop".to_string()),
                usage: Some(Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    ..Default::default()
                }),
                tool_calls: None,
            },
        ];

        let backend = Arc::new(StreamingBackend { chunks });
        let mut agent = Agent::new(
            AgentConfig {
                model: "test".to_string(),
                ..Default::default()
            },
            backend,
            ToolRegistry::new(),
            ToolContext::new(std::env::temp_dir()),
        );

        let collected = Arc::new(Mutex::new(Vec::new()));
        agent.set_event_sink(CollectingSink {
            events: collected.clone(),
        });

        let outcome = agent.run_turn("Say hello").await.unwrap();

        let events = collected.lock().unwrap();
        let deltas: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::AssistantDelta { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deltas, vec!["Hello ", "World", "!"]);
        assert_eq!(outcome.final_response, "Hello World!");
    }

    /// Mock ChatBackend that yields 1 chunk then hangs forever.
    struct HangingBackend;

    #[async_trait]
    impl ChatBackend for HangingBackend {
        async fn chat(
            &self,
            _api_key_id: &str,
            _request: ChatRequest,
        ) -> Result<ChatResponse, HyperInferError> {
            Err(HyperInferError::Config(std::io::Error::other(
                "chat() not implemented",
            )))
        }

        async fn chat_stream(
            &self,
            _api_key_id: &str,
            _request: ChatRequest,
        ) -> Result<ChatStream, HyperInferError> {
            use futures::StreamExt;
            let first = hyperinfer_core::ChatChunk {
                id: "hang-1".to_string(),
                model: "test".to_string(),
                delta: "Starting...".to_string(),
                finish_reason: None,
                usage: None,
                tool_calls: None,
            };
            let stream =
                futures::stream::once(async move { Ok(first) }).chain(futures::stream::pending());
            Ok(Box::pin(stream))
        }
    }

    #[tokio::test]
    async fn test_cancellation_aborts_streaming_turn() {
        let backend = Arc::new(HangingBackend);
        let mut agent = Agent::new(
            AgentConfig {
                model: "test".to_string(),
                ..Default::default()
            },
            backend,
            ToolRegistry::new(),
            ToolContext::new(std::env::temp_dir()),
        );

        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel_clone = cancel.clone();

        let agent_task = tokio::spawn(async move {
            agent
                .run_turn_with_cancel("Do something long", cancel_clone)
                .await
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        cancel.cancel();

        let result = tokio::time::timeout(std::time::Duration::from_secs(2), agent_task)
            .await
            .expect("Agent task should complete promptly after cancellation")
            .expect("Agent task should not panic");

        assert!(
            matches!(result, Err(AgentError::Cancelled)),
            "Expected AgentError::Cancelled, got {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_host_runner_run_cancellable_kills_command() {
        let ctx = ToolContext::new(std::env::temp_dir());
        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel_clone = cancel.clone();

        let start = std::time::Instant::now();

        let cancel_task = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            cancel_clone.cancel();
        });

        let result = HostRunner
            .run_cancellable(
                "sleep 10",
                &ctx.workspace_root,
                std::time::Duration::from_secs(10),
                &ctx.sandbox_policy,
                cancel,
            )
            .await;

        let _ = cancel_task.await;
        let elapsed = start.elapsed();

        assert!(result.is_err(), "Expected error from cancelled command");
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.to_lowercase().contains("canceled")
                || err_msg.to_lowercase().contains("cancelled"),
            "Error message should mention cancellation: {}",
            err_msg
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "Command should be killed quickly, took {:?}",
            elapsed
        );
    }

    /// Mock read-only tool that sleeps and returns its input ID.
    struct SlowReadOnlyTool;

    #[async_trait]
    impl Tool for SlowReadOnlyTool {
        fn name(&self) -> &str {
            "slow_read"
        }

        fn definition(&self) -> hyperinfer_core::ToolDefinition {
            hyperinfer_core::ToolDefinition {
                name: "slow_read".to_string(),
                description: "A slow read-only tool for testing concurrency".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "string" }
                    },
                    "required": ["id"]
                }),
            }
        }

        fn requires_approval(&self) -> bool {
            false
        }

        async fn execute(&self, args: serde_json::Value, _ctx: &ToolContext) -> ToolOutput {
            let id = match args["id"].as_str() {
                Some(s) => s,
                None => return ToolOutput::error("Missing 'id' argument"),
            };
            tokio::time::sleep(std::time::Duration::from_millis(120)).await;
            ToolOutput::success(format!("result-{}", id))
        }
    }

    #[tokio::test]
    async fn test_concurrent_read_only_tools_preserve_order() {
        let backend = Arc::new(ScriptedBackend::new(vec![
            ChatMessage::assistant_with_tools(
                "Running concurrent reads",
                vec![
                    ToolCall {
                        id: "call_a".to_string(),
                        name: "slow_read".to_string(),
                        arguments: serde_json::json!({ "id": "a" }).to_string(),
                    },
                    ToolCall {
                        id: "call_b".to_string(),
                        name: "slow_read".to_string(),
                        arguments: serde_json::json!({ "id": "b" }).to_string(),
                    },
                    ToolCall {
                        id: "call_c".to_string(),
                        name: "slow_read".to_string(),
                        arguments: serde_json::json!({ "id": "c" }).to_string(),
                    },
                ],
            ),
            ChatMessage::assistant("All reads complete."),
        ]));

        let mut registry = ToolRegistry::new();
        registry.register(SlowReadOnlyTool);

        let mut agent = Agent::new(
            AgentConfig {
                model: "test".to_string(),
                ..Default::default()
            },
            backend,
            registry,
            ToolContext::new(std::env::temp_dir()),
        );

        let collected = Arc::new(Mutex::new(Vec::new()));
        agent.set_event_sink(CollectingSink {
            events: collected.clone(),
        });

        let start = std::time::Instant::now();
        let outcome = agent.run_turn("Run concurrent reads").await.unwrap();
        let elapsed = start.elapsed();

        assert!(outcome.final_response.contains("All reads complete"));
        assert!(
            elapsed < std::time::Duration::from_millis(300),
            "3x 120ms read-only tools should run concurrently (<300ms), took {:?}",
            elapsed
        );

        let events = collected.lock().unwrap();
        let ids: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ToolExecuted { tool_call, output } => {
                    Some(format!("{}:{}", tool_call.id, output.content))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec!["call_a:result-a", "call_b:result-b", "call_c:result-c"]
        );
    }

    #[tokio::test]
    async fn test_chat_only_mode_omits_tools_and_cli_parses_chat() {
        let opts = CliOptions::parse_from(["--approval", "chat"]).unwrap();
        assert_eq!(opts.approval_mode, ApprovalMode::ChatOnly);

        let backend = Arc::new(ScriptedBackend::new(vec![ChatMessage::assistant("Done")]));

        let mut agent = Agent::new(
            AgentConfig {
                model: "test".to_string(),
                approval_mode: ApprovalMode::ChatOnly,
                ..Default::default()
            },
            backend.clone(),
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(std::env::temp_dir()),
        );

        let outcome = agent.run_turn("Hello").await.unwrap();
        assert_eq!(outcome.final_response, "Done");

        let requests = backend.seen_requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(
            requests[0].tools.is_none(),
            "ChatOnly mode should omit tool definitions from ChatRequest"
        );
    }

    #[tokio::test]
    async fn test_protected_metadata_paths_block_direct_and_symlink_writes() {
        let tmp_dir = TempDir::new().unwrap();
        let tmp = tmp_dir.path();

        // Create protected directories with some content
        let git_dir = tmp.join(".git");
        let hooks_dir = git_dir.join("hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();
        std::fs::write(git_dir.join("config"), "original git config").unwrap();

        let hc_dir = tmp.join(".hypercode");
        std::fs::create_dir_all(hc_dir.join("logs")).unwrap();
        std::fs::write(hc_dir.join("hooks.json"), "original hooks").unwrap();
        std::fs::write(hc_dir.join("logs/sample.log"), "log content").unwrap();

        let ctx = ToolContext::new(tmp);
        let registry = ToolRegistry::with_default_coding_tools();

        // write_file to .git/hooks/pre-commit should fail
        let result = registry
            .execute(
                &make_call(
                    "write_file",
                    serde_json::json!({ "path": ".git/hooks/pre-commit", "content": "echo hi" }),
                ),
                &ctx,
            )
            .await;
        assert!(result.is_error, "write_file to .git/hooks should fail");
        assert!(
            !hooks_dir.join("pre-commit").exists(),
            "file should not be created"
        );

        // write_file to .hypercode/config.toml should fail
        let result = registry
            .execute(
                &make_call(
                    "write_file",
                    serde_json::json!({ "path": ".hypercode/config.toml", "content": "x=1" }),
                ),
                &ctx,
            )
            .await;
        assert!(
            result.is_error,
            "write_file to .hypercode/config.toml should fail"
        );
        assert!(
            !hc_dir.join("config.toml").exists(),
            "file should not be created"
        );

        // edit_file on .git/config should fail
        let result = registry
            .execute(
                &make_call(
                    "edit_file",
                    serde_json::json!({
                        "path": ".git/config",
                        "target": "original git config",
                        "replacement": "modified"
                    }),
                ),
                &ctx,
            )
            .await;
        assert!(result.is_error, "edit_file on .git/config should fail");
        assert_eq!(
            std::fs::read_to_string(git_dir.join("config")).unwrap(),
            "original git config"
        );

        // edit_file on .hypercode/hooks.json should fail
        let result = registry
            .execute(
                &make_call(
                    "edit_file",
                    serde_json::json!({
                        "path": ".hypercode/hooks.json",
                        "target": "original hooks",
                        "replacement": "modified"
                    }),
                ),
                &ctx,
            )
            .await;
        assert!(
            result.is_error,
            "edit_file on .hypercode/hooks.json should fail"
        );
        assert_eq!(
            std::fs::read_to_string(hc_dir.join("hooks.json")).unwrap(),
            "original hooks"
        );

        // read_file on .hypercode/logs/sample.log should still succeed
        let result = registry
            .execute(
                &make_call(
                    "read_file",
                    serde_json::json!({ "path": ".hypercode/logs/sample.log" }),
                ),
                &ctx,
            )
            .await;
        assert!(
            !result.is_error,
            "read_file on .hypercode/logs should succeed"
        );
        assert!(result.content.contains("log content"));

        // Symlink attack: create a symlink inside workspace pointing to .git/hooks
        #[cfg(unix)]
        {
            let evil_link = tmp.join("evil_link");
            std::os::unix::fs::symlink(&hooks_dir, &evil_link).unwrap();
            let result = registry
                .execute(
                    &make_call(
                        "write_file",
                        serde_json::json!({ "path": "evil_link/pre-commit", "content": "echo hi" }),
                    ),
                    &ctx,
                )
                .await;
            assert!(
                result.is_error,
                "write_file via symlink to .git/hooks should fail"
            );
            assert!(
                !hooks_dir.join("pre-commit").exists(),
                "file should not be created via symlink"
            );
        }
    }

    #[test]
    fn test_openshell_policy_yaml_marks_git_and_hypercode_read_only() {
        let policy = SandboxPolicy::restricted_for_workspace("/workspace/proj");
        let yaml = policy.to_openshell_policy_yaml();
        assert!(
            yaml.contains("/workspace/proj/.git"),
            "YAML should include .git as read-only"
        );
        assert!(
            yaml.contains("/workspace/proj/.hypercode"),
            "YAML should include .hypercode as read-only"
        );
    }

    #[tokio::test]
    async fn test_agent_escalates_protected_path_even_under_auto_approve() {
        let tmp = std::env::temp_dir().join("hypercode-agent-protected-test");
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::create_dir_all(tmp.join(".git/hooks")).unwrap();

        let backend = Arc::new(ScriptedBackend::new(vec![
            ChatMessage::assistant_with_tools(
                "Writing to protected path",
                vec![ToolCall {
                    id: "call_protected".to_string(),
                    name: "write_file".to_string(),
                    arguments: serde_json::json!({
                        "path": ".git/hooks/pre-commit",
                        "content": "echo hi"
                    })
                    .to_string(),
                }],
            ),
            ChatMessage::assistant("Done after rejection."),
        ]));

        let mut agent = Agent::new(
            AgentConfig {
                model: "test".to_string(),
                approval_mode: ApprovalMode::AutoApprove,
                ..Default::default()
            },
            backend,
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(&tmp),
        );
        agent.set_approval_handler(RejectAllHandler);

        let collected = Arc::new(Mutex::new(Vec::new()));
        agent.set_event_sink(CollectingSink {
            events: collected.clone(),
        });

        let outcome = agent.run_turn("Write to protected path").await.unwrap();
        assert!(outcome.final_response.contains("Done after rejection"));

        let events = collected.lock().unwrap();
        let approval_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, AgentEvent::ToolApprovalChecked { .. }))
            .collect();
        assert!(
            !approval_events.is_empty(),
            "Expected ToolApprovalChecked event for protected path"
        );
        let approval = &approval_events[0];
        match approval {
            AgentEvent::ToolApprovalChecked { approved, .. } => {
                assert!(!approved, "Protected path should not be auto-approved");
            }
            _ => unreachable!(),
        }

        assert!(
            !tmp.join(".git/hooks/pre-commit").exists(),
            "File should not be created after rejection"
        );
    }

    #[tokio::test]
    async fn test_agent_approved_protected_path_write_succeeds_when_explicitly_approved() {
        let tmp_dir = TempDir::new().unwrap();
        let hooks_dir = tmp_dir.path().join(".git/hooks");
        std::fs::create_dir_all(&hooks_dir).unwrap();

        let backend = Arc::new(ScriptedBackend::new(vec![
            ChatMessage::assistant_with_tools(
                "Writing hook with approval",
                vec![make_call(
                    "write_file",
                    serde_json::json!({
                        "path": ".git/hooks/pre-commit",
                        "content": "#!/bin/sh\necho ok\n"
                    }),
                )],
            ),
            ChatMessage::assistant("Done."),
        ]));

        let mut agent = Agent::new(
            AgentConfig {
                model: "test".to_string(),
                approval_mode: ApprovalMode::AutoApprove,
                ..Default::default()
            },
            backend,
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(tmp_dir.path()),
        )
        .with_approval_handler(AutoApproveHandler);

        let collected = Arc::new(Mutex::new(Vec::new()));
        agent.set_event_sink(CollectingSink {
            events: collected.clone(),
        });

        let outcome = agent.run_turn("Add pre-commit hook").await.unwrap();
        assert_eq!(outcome.final_response, "Done.");

        let events = collected.lock().unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, AgentEvent::ToolApprovalChecked { approved: true, .. })));
        assert_eq!(
            std::fs::read_to_string(hooks_dir.join("pre-commit")).unwrap(),
            "#!/bin/sh\necho ok\n"
        );
    }

    // --- Model context profile tests ---

    #[test]
    fn test_model_context_profile_claude_opus() {
        let p = model_context_profile("claude-opus-4-20250514");
        assert_eq!(p.context_window, 200_000);
        assert_eq!(p.input_cost_per_million, 15.0);
        assert_eq!(p.output_cost_per_million, 75.0);
        assert_eq!(p.cache_read_cost_per_million, 1.50);
        assert_eq!(p.cache_creation_cost_per_million, 18.75);
    }

    #[test]
    fn test_model_context_profile_claude_haiku() {
        let p = model_context_profile("claude-haiku-3-5-20241022");
        assert_eq!(p.context_window, 200_000);
        assert_eq!(p.input_cost_per_million, 0.80);
        assert_eq!(p.output_cost_per_million, 4.00);
    }

    #[test]
    fn test_model_context_profile_claude_sonnet_default() {
        let p = model_context_profile("claude-sonnet-4-20250514");
        assert_eq!(p.context_window, 200_000);
        assert_eq!(p.input_cost_per_million, 3.00);
        assert_eq!(p.output_cost_per_million, 15.00);
    }

    #[test]
    fn test_model_context_profile_gpt_4o() {
        let p = model_context_profile("gpt-4o");
        assert_eq!(p.context_window, 128_000);
        assert_eq!(p.input_cost_per_million, 2.50);
        assert_eq!(p.output_cost_per_million, 10.00);
    }

    #[test]
    fn test_model_context_profile_gpt_4o_mini() {
        let p = model_context_profile("gpt-4o-mini");
        assert_eq!(p.context_window, 128_000);
        assert_eq!(p.input_cost_per_million, 0.15);
        assert_eq!(p.output_cost_per_million, 0.60);
    }

    #[test]
    fn test_model_context_profile_gemini() {
        let p = model_context_profile("gemini-2.5-pro");
        assert_eq!(p.context_window, 1_000_000);
        assert_eq!(p.input_cost_per_million, 1.25);
        assert_eq!(p.output_cost_per_million, 5.00);
    }

    #[test]
    fn test_model_context_profile_local_model() {
        let p = model_context_profile("qwen2.5-coder:latest");
        assert_eq!(p.context_window, 32_000);
        assert_eq!(p.input_cost_per_million, 0.0);
        assert_eq!(p.output_cost_per_million, 0.0);
    }

    #[test]
    fn test_model_context_profile_from_live_catalog() {
        let raw = r#"{
            "anthropic": {
                "models": {
                    "claude-fable-5": {
                        "id": "claude-fable-5",
                        "name": "Claude Fable 5",
                        "limit": { "context": 1000000, "output": 128000 },
                        "cost": { "input": 10.0, "output": 50.0, "cache_read": 1.0, "cache_write": 12.5 }
                    }
                }
            }
        }"#;
        let catalog = hyperinfer_core::ModelCatalog::from_models_dev_json(raw).unwrap();
        let p = model_context_profile_from_catalog(&catalog, "claude-fable-5").unwrap();
        assert_eq!(p.context_window, 1_000_000);
        assert_eq!(p.input_cost_per_million, 10.0);
        assert_eq!(p.output_cost_per_million, 50.0);
        assert_eq!(p.cache_read_cost_per_million, 1.0);
        assert_eq!(p.cache_creation_cost_per_million, 12.5);
    }

    // --- CompactionConfig effective_max_context_tokens tests ---

    #[test]
    fn test_compaction_config_auto_model_budget() {
        let config = CompactionConfig::default();
        assert!(config.auto_model_budget);
        assert_eq!(config.compaction_ratio, 0.75);
        // Claude Sonnet: 200_000 * 0.75 = 150_000
        assert_eq!(
            config.effective_max_context_tokens("claude-sonnet-4-20250514"),
            150_000
        );
        // GPT-4o: 128_000 * 0.75 = 96_000
        assert_eq!(config.effective_max_context_tokens("gpt-4o"), 96_000);
    }

    #[test]
    fn test_compaction_config_explicit_override() {
        let config = CompactionConfig {
            max_context_tokens: 50_000,
            auto_model_budget: false,
            ..Default::default()
        };
        assert_eq!(
            config.effective_max_context_tokens("claude-sonnet-4-20250514"),
            50_000
        );
        assert_eq!(config.effective_max_context_tokens("gpt-4o"), 50_000);
    }

    // --- TokenAccounting tests ---

    #[test]
    fn test_token_accounting_record_usage() {
        let mut ta = TokenAccounting::default();
        let usage = Usage {
            input_tokens: 1000,
            output_tokens: 200,
            cache_read_tokens: Some(500),
            cache_creation_tokens: Some(100),
        };
        ta.record_usage(&usage, 5);
        assert_eq!(ta.input_tokens, 1000);
        assert_eq!(ta.output_tokens, 200);
        assert_eq!(ta.cache_read_tokens, 500);
        assert_eq!(ta.cache_creation_tokens, 100);
        assert_eq!(ta.last_prompt_tokens, Some(1600));
        assert_eq!(ta.last_history_len, 5);
    }

    #[test]
    fn test_token_accounting_calibrated_context_tokens() {
        let mut ta = TokenAccounting::default();
        let usage = Usage {
            input_tokens: 1000,
            output_tokens: 200,
            cache_read_tokens: Some(500),
            cache_creation_tokens: Some(100),
        };
        ta.record_usage(&usage, 5);

        // No new messages: should return base
        let history: Vec<ChatMessage> = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("u1"),
            ChatMessage::assistant("a1"),
            ChatMessage::user("u2"),
            ChatMessage::assistant("a2"),
        ];
        let tokens = ta.calibrated_context_tokens(&history);
        assert_eq!(tokens, 1600);

        // New messages added: should add their estimate
        let history2: Vec<ChatMessage> = history
            .into_iter()
            .chain(std::iter::once(ChatMessage::user("new user message")))
            .collect();
        let tokens2 = ta.calibrated_context_tokens(&history2);
        assert!(tokens2 > 1600);
    }

    #[test]
    fn test_token_accounting_context_utilization_pct() {
        let mut ta = TokenAccounting::default();
        let usage = Usage {
            input_tokens: 10000,
            output_tokens: 1000,
            cache_read_tokens: None,
            cache_creation_tokens: None,
        };
        ta.record_usage(&usage, 3);

        let history: Vec<ChatMessage> = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("u"),
            ChatMessage::assistant("a"),
        ];
        // Claude Sonnet: 200_000 window, 10_000 prompt tokens (input only)
        let pct = ta.context_utilization_pct("claude-sonnet-4-20250514", &history);
        assert!((pct - 5.0).abs() < 0.1);
    }

    #[test]
    fn test_token_accounting_estimated_cost_usd() {
        let mut ta = TokenAccounting::default();
        let usage = Usage {
            input_tokens: 1000000,
            output_tokens: 500000,
            cache_read_tokens: Some(200000),
            cache_creation_tokens: Some(100000),
        };
        ta.record_usage(&usage, 3);

        // Claude Sonnet: $3.00/M input, $15.00/M output, $0.30/M cache read, $3.75/M cache write
        let cost = ta.estimated_cost_usd("claude-sonnet-4-20250514");
        // 1M * 3.00 + 0.5M * 15.00 + 0.2M * 0.30 + 0.1M * 3.75 = 3.00 + 7.50 + 0.06 + 0.375 = 10.935
        assert!((cost - 10.935).abs() < 0.001);
    }

    // --- deduplicate_read_file_outputs tests ---

    fn make_read_call(id: &str, path: &str) -> ChatMessage {
        ChatMessage::assistant_with_tools(
            "",
            vec![ToolCall {
                id: id.to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({ "path": path }).to_string(),
            }],
        )
    }

    fn make_edit_call(id: &str, path: &str) -> ChatMessage {
        ChatMessage::assistant_with_tools(
            "",
            vec![ToolCall {
                id: id.to_string(),
                name: "edit_file".to_string(),
                arguments: serde_json::json!({ "path": path }).to_string(),
            }],
        )
    }

    fn make_tool_result(id: &str, content: &str) -> ChatMessage {
        ChatMessage::tool_result(id.to_string(), "read_file", content.to_string())
    }

    #[test]
    fn test_deduplicate_superseded_reread() {
        let mut history = vec![
            make_read_call("call1", "foo.txt"),
            make_tool_result("call1", "old content"),
            make_read_call("call2", "foo.txt"),
            make_tool_result("call2", "new content"),
        ];
        let deduped = deduplicate_read_file_outputs(&mut history);
        assert_eq!(deduped, 1);
        assert!(history[1].content.starts_with("[Superseded:"));
        assert_eq!(history[3].content, "new content");
    }

    #[test]
    fn test_deduplicate_superseded_by_edit() {
        let mut history = vec![
            make_read_call("call1", "foo.txt"),
            make_tool_result("call1", "original content"),
            make_edit_call("call2", "foo.txt"),
            make_tool_result("call2", "edited"),
        ];
        let deduped = deduplicate_read_file_outputs(&mut history);
        assert_eq!(deduped, 1);
        assert!(history[1].content.starts_with("[Superseded:"));
    }

    #[test]
    fn test_deduplicate_preserves_latest_read() {
        let mut history = vec![
            make_read_call("call1", "foo.txt"),
            make_tool_result("call1", "first"),
            make_read_call("call2", "foo.txt"),
            make_tool_result("call2", "second"),
            make_read_call("call3", "foo.txt"),
            make_tool_result("call3", "third"),
        ];
        let deduped = deduplicate_read_file_outputs(&mut history);
        assert_eq!(deduped, 2);
        assert!(history[1].content.starts_with("[Superseded:"));
        assert!(history[3].content.starts_with("[Superseded:"));
        assert_eq!(history[5].content, "third");
    }

    #[test]
    fn test_deduplicate_different_files() {
        let mut history = vec![
            make_read_call("call1", "foo.txt"),
            make_tool_result("call1", "foo content"),
            make_read_call("call2", "bar.txt"),
            make_tool_result("call2", "bar content"),
        ];
        let deduped = deduplicate_read_file_outputs(&mut history);
        assert_eq!(deduped, 0);
        assert_eq!(history[1].content, "foo content");
        assert_eq!(history[3].content, "bar content");
    }

    #[test]
    fn test_deduplicate_ignores_failed_or_denied_later_call() {
        let mut history = vec![
            make_read_call("call1", "foo.txt"),
            make_tool_result("call1", "important content"),
            make_edit_call("call2", "foo.txt"),
            make_tool_result(
                "call2",
                "Tool execution denied by user approval policy (requires approval).",
            ),
            make_read_call("call3", "foo.txt"),
            make_tool_result("call3", "Error: failed to read file"),
        ];
        let deduped = deduplicate_read_file_outputs(&mut history);
        assert_eq!(deduped, 0);
        assert_eq!(history[1].content, "important content");
    }

    // --- Agent prompt_caching and token_accounting integration test ---

    #[tokio::test]
    async fn test_agent_sets_prompt_caching_and_records_usage() {
        struct CacheUsageBackend {
            seen_requests: Mutex<Vec<ChatRequest>>,
        }

        #[async_trait]
        impl ChatBackend for CacheUsageBackend {
            async fn chat(
                &self,
                _key: &str,
                request: ChatRequest,
            ) -> Result<ChatResponse, HyperInferError> {
                self.seen_requests.lock().unwrap().push(request.clone());
                Ok(ChatResponse {
                    id: "cache-resp".to_string(),
                    model: request.model,
                    choices: vec![Choice {
                        index: 0,
                        message: ChatMessage::assistant("Done"),
                        finish_reason: Some("stop".to_string()),
                    }],
                    usage: Usage {
                        input_tokens: 10,
                        output_tokens: 5,
                        cache_read_tokens: Some(2),
                        cache_creation_tokens: Some(1),
                    },
                })
            }
        }

        let backend = Arc::new(CacheUsageBackend {
            seen_requests: Mutex::new(Vec::new()),
        });

        let mut agent = Agent::new(
            AgentConfig {
                model: "claude-sonnet-4-20250514".to_string(),
                ..Default::default()
            },
            backend.clone(),
            ToolRegistry::new(),
            ToolContext::new(std::env::temp_dir()),
        );

        let outcome = agent.run_turn("Hello").await.unwrap();
        assert_eq!(outcome.final_response, "Done");

        // Verify prompt_caching was set
        let requests = backend.seen_requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].prompt_caching, Some(true));

        // Verify token accounting was updated (including cache tokens)
        let ta = agent.token_accounting();
        assert_eq!(ta.input_tokens, 10);
        assert_eq!(ta.output_tokens, 5);
        assert_eq!(ta.cache_read_tokens, 2);
        assert_eq!(ta.cache_creation_tokens, 1);
    }

    // --- Architect + Editor Multi-Model Split Tests ---

    #[test]
    fn test_effective_compaction_model_fallback_chain() {
        let mut cfg = AgentConfig {
            model: "claude-opus-4".to_string(),
            ..Default::default()
        };
        assert_eq!(cfg.effective_compaction_model(), "claude-opus-4");

        cfg.editor_model = Some("claude-haiku-3-5".to_string());
        assert_eq!(cfg.effective_compaction_model(), "claude-haiku-3-5");

        cfg.compaction_model = Some("gpt-4o-mini".to_string());
        assert_eq!(cfg.effective_compaction_model(), "gpt-4o-mini");
    }

    #[tokio::test]
    async fn test_compaction_dispatches_to_compaction_and_editor_models() {
        // Explicit compaction_model takes precedence over editor_model
        let backend = Arc::new(ScriptedBackend::new(vec![ChatMessage::assistant(
            "Summary of prior turns",
        )]));
        let mut agent = Agent::new(
            AgentConfig {
                model: "claude-opus-4".to_string(),
                editor_model: Some("claude-haiku-3-5".to_string()),
                compaction_model: Some("gpt-4o-mini".to_string()),
                compaction: CompactionConfig {
                    keep_recent_messages: 2,
                    ..Default::default()
                },
                ..Default::default()
            },
            backend.clone(),
            ToolRegistry::new(),
            ToolContext::new(std::env::temp_dir()),
        );
        agent.push_message(ChatMessage::user("turn 1"));
        agent.push_message(ChatMessage::assistant("reply 1"));
        agent.push_message(ChatMessage::user("turn 2"));
        agent.push_message(ChatMessage::assistant("reply 2"));

        assert!(agent.compact().await.unwrap());
        {
            let seen = backend.seen_requests.lock().unwrap();
            assert_eq!(seen.len(), 1);
            assert_eq!(seen[0].model, "gpt-4o-mini");
        }
        assert!(agent
            .token_accounting()
            .per_model
            .contains_key("gpt-4o-mini"));

        // Without compaction_model, falls back to editor_model
        let backend2 = Arc::new(ScriptedBackend::new(vec![ChatMessage::assistant(
            "Summary via editor",
        )]));
        let mut agent2 = Agent::new(
            AgentConfig {
                model: "claude-opus-4".to_string(),
                editor_model: Some("claude-haiku-3-5".to_string()),
                compaction_model: None,
                compaction: CompactionConfig {
                    keep_recent_messages: 2,
                    ..Default::default()
                },
                ..Default::default()
            },
            backend2.clone(),
            ToolRegistry::new(),
            ToolContext::new(std::env::temp_dir()),
        );
        agent2.push_message(ChatMessage::user("turn 1"));
        agent2.push_message(ChatMessage::assistant("reply 1"));
        agent2.push_message(ChatMessage::user("turn 2"));
        agent2.push_message(ChatMessage::assistant("reply 2"));

        assert!(agent2.compact().await.unwrap());
        let seen2 = backend2.seen_requests.lock().unwrap();
        assert_eq!(seen2.len(), 1);
        assert_eq!(seen2[0].model, "claude-haiku-3-5");
    }

    #[tokio::test]
    async fn test_multi_model_architect_editor_dispatch_and_per_model_cost() {
        let tmp = TempDir::new().unwrap();
        let backend = Arc::new(ScriptedBackend::new(vec![
            ChatMessage::assistant_with_tools(
                "Creating file",
                vec![make_call(
                    "write_file",
                    serde_json::json!({ "path": "hello.txt", "content": "hi\n" }),
                )],
            ),
            ChatMessage::assistant("File written."),
        ]));

        let mut agent = Agent::new(
            AgentConfig {
                model: "claude-opus-4".to_string(),
                editor_model: Some("claude-haiku-3-5".to_string()),
                approval_mode: ApprovalMode::AutoApprove,
                ..Default::default()
            },
            backend.clone(),
            ToolRegistry::with_default_coding_tools(),
            ToolContext::new(tmp.path()),
        );

        let outcome = agent.run_turn("Create hello.txt").await.unwrap();
        assert_eq!(outcome.final_response, "File written.");

        let seen = backend.seen_requests.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].model, "claude-opus-4");
        assert_eq!(seen[1].model, "claude-haiku-3-5");

        let ta = agent.token_accounting();
        assert_eq!(ta.per_model.len(), 2);
        assert_eq!(ta.per_model["claude-opus-4"].requests, 1);
        assert_eq!(ta.per_model["claude-haiku-3-5"].requests, 1);
        assert!(ta.total_cost_usd("claude-opus-4") > 0.0);
    }

    #[test]
    fn test_cli_multi_model_flags_and_model_repl_command() {
        let opts = CliOptions::parse_from([
            "--architect-model",
            "claude-opus-4",
            "--editor-model",
            "claude-haiku-3-5",
            "--compaction-model",
            "gpt-4o-mini",
        ])
        .unwrap();
        assert_eq!(opts.model.as_deref(), Some("claude-opus-4"));
        assert_eq!(opts.editor_model.as_deref(), Some("claude-haiku-3-5"));
        assert_eq!(opts.compaction_model.as_deref(), Some("gpt-4o-mini"));

        assert_eq!(
            ReplCommand::parse("/model").unwrap().unwrap(),
            ReplCommand::Model {
                role: None,
                model: None,
            }
        );
        assert_eq!(
            ReplCommand::parse("/model gpt-4o").unwrap().unwrap(),
            ReplCommand::Model {
                role: None,
                model: Some("gpt-4o".to_string()),
            }
        );
        assert_eq!(
            ReplCommand::parse("/model editor qwen2.5-coder")
                .unwrap()
                .unwrap(),
            ReplCommand::Model {
                role: Some("editor".to_string()),
                model: Some("qwen2.5-coder".to_string()),
            }
        );
        assert_eq!(
            ReplCommand::parse("/model compaction claude-haiku")
                .unwrap()
                .unwrap(),
            ReplCommand::Model {
                role: Some("compaction".to_string()),
                model: Some("claude-haiku".to_string()),
            }
        );
    }
}
