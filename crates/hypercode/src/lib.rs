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
    ChatBackend, EventSink, NoopEventSink, RejectAllHandler, TurnOutcome,
};
pub use cli::{
    build_client_and_model, format_tool_call_preview, run_cli, CliOptions, ReplCommand,
    TerminalApprovalHandler, TerminalEventSink,
};
pub use context::{
    compact_history_with_llm, estimate_messages_tokens, estimate_tokens,
    find_safe_compaction_split, prune_old_tool_outputs, CompactionConfig, WorkspaceContext,
    INSTRUCTION_FILES,
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
}
