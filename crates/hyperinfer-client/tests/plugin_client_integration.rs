//! End-to-end HyperInferClient GatewayPlugin & RedactionPlugin integration tests (HYP-67).

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures::Stream;
use hyperinfer_client::HyperInferClient;
use hyperinfer_core::{
    ChatChunk, ChatMessage, ChatRequest, ChatResponse, Choice, Config, ContentPart, GatewayPlugin,
    HyperInferError, PluginContext, Provider, RedactionPlugin, ToolCall, Usage,
};
use hyperinfer_providers::{LlmProvider, ProviderRegistry};
use tokio::sync::RwLock;

#[derive(Clone)]
struct MockClientProvider {
    seen_requests: Arc<RwLock<Vec<ChatRequest>>>,
    fail_next: Arc<AtomicBool>,
}

#[async_trait]
impl LlmProvider for MockClientProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat(
        &self,
        request: &ChatRequest,
        _api_key: &str,
    ) -> Result<ChatResponse, HyperInferError> {
        self.seen_requests.write().await.push(request.clone());
        if self.fail_next.load(Ordering::SeqCst) {
            return Err(HyperInferError::ApiError {
                status: 503,
                message: "upstream unavailable".to_string(),
            });
        }
        let mut msg = ChatMessage::assistant_with_tools(
            "Contact bob@corp.io with key sk-ant-abcdefghijklmnop",
            vec![ToolCall {
                id: "call_1".to_string(),
                name: "send_mail".to_string(),
                arguments: r#"{"token":"Bearer supersecrettoken123"}"#.to_string(),
            }],
        );
        msg.content_parts = Some(vec![ContentPart::Text {
            text: "SSN is 123-45-6789".to_string(),
        }]);
        Ok(ChatResponse {
            id: "resp-client-1".to_string(),
            model: request.model.clone(),
            choices: vec![Choice {
                index: 0,
                message: msg,
                finish_reason: Some("stop".to_string()),
            }],
            usage: Usage {
                input_tokens: 10,
                output_tokens: 10,
                ..Default::default()
            },
        })
    }

    fn stream(
        &self,
        _request: &ChatRequest,
        _api_key: &str,
    ) -> Pin<Box<dyn Stream<Item = Result<ChatChunk, HyperInferError>> + Send + 'static>> {
        Box::pin(futures::stream::empty())
    }
}

struct ErrorTrackerPlugin {
    errors: Arc<RwLock<Vec<String>>>,
}

#[async_trait]
impl GatewayPlugin for ErrorTrackerPlugin {
    fn name(&self) -> &str {
        "error_tracker"
    }

    async fn on_error(&self, _ctx: &mut PluginContext, err: &str) {
        self.errors.write().await.push(err.to_string());
    }
}

#[tokio::test]
async fn test_client_redaction_propagates_request_and_response_and_tracks_errors() {
    let seen = Arc::new(RwLock::new(Vec::new()));
    let fail_next = Arc::new(AtomicBool::new(false));
    let errors = Arc::new(RwLock::new(Vec::new()));

    let mut api_keys = HashMap::new();
    api_keys.insert("openai".to_string(), "sk-dummy-provider-key".to_string());
    let config = Config {
        api_keys,
        routing_rules: Vec::new(),
        quotas: HashMap::new(),
        model_aliases: HashMap::new(),
        default_provider: Some(Provider::OpenAI),
    };

    let registry = Arc::new(ProviderRegistry::new());
    registry.register(MockClientProvider {
        seen_requests: seen.clone(),
        fail_next: fail_next.clone(),
    });

    let mut client = HyperInferClient::new_local(config)
        .await
        .unwrap()
        .with_plugin(Arc::new(RedactionPlugin::with_defaults()))
        .with_plugin(Arc::new(ErrorTrackerPlugin {
            errors: errors.clone(),
        }));
    client.inject_provider_registry(registry).await;

    // 1. Successful call: verify request AND response (content, content_parts, tool_calls) are redacted
    let req = ChatRequest {
        model: "gpt-4o".to_string(),
        messages: vec![ChatMessage::assistant_with_tools(
            "My email is alice@example.com",
            vec![ToolCall {
                id: "call_req".to_string(),
                name: "auth".to_string(),
                arguments: r#"{"api_key":"sk-proj-1234567890abcdef"}"#.to_string(),
            }],
        )],
        ..Default::default()
    };

    let resp = client.chat("virtual-key-1", req).await.unwrap();

    // Upstream saw redacted request & tool call args
    let recorded = seen.read().await;
    assert_eq!(recorded.len(), 1);
    assert!(recorded[0].messages[0].content.contains("[REDACTED_EMAIL]"));
    assert!(!recorded[0].messages[0]
        .content
        .contains("alice@example.com"));
    let req_tool_args = &recorded[0].messages[0].tool_calls.as_ref().unwrap()[0].arguments;
    assert!(req_tool_args.contains("[REDACTED_API_KEY]"));
    assert!(!req_tool_args.contains("sk-proj-1234567890abcdef"));
    drop(recorded);

    // Client received redacted response (content, content_parts, tool_calls)
    let resp_msg = &resp.choices[0].message;
    assert!(resp_msg.content.contains("[REDACTED_EMAIL]"));
    assert!(resp_msg.content.contains("[REDACTED_API_KEY]"));
    assert!(!resp_msg.content.contains("bob@corp.io"));
    assert!(!resp_msg.content.contains("sk-ant-abcdefghijklmnop"));
    let resp_tool_args = &resp_msg.tool_calls.as_ref().unwrap()[0].arguments;
    assert!(resp_tool_args.contains("Bearer [REDACTED_TOKEN]"));
    assert!(!resp_tool_args.contains("supersecrettoken123"));
    if let ContentPart::Text { text } = &resp_msg.content_parts.as_ref().unwrap()[0] {
        assert!(text.contains("[REDACTED_SSN]"));
        assert!(!text.contains("123-45-6789"));
    } else {
        panic!("expected text content part");
    }

    // 2. Failing call: verify on_error plugin hook fires
    fail_next.store(true, Ordering::SeqCst);
    let err_req = ChatRequest {
        model: "gpt-4o".to_string(),
        messages: vec![ChatMessage::user("trigger error")],
        ..Default::default()
    };
    assert!(client.chat("virtual-key-1", err_req).await.is_err());
    let seen_errors = errors.read().await;
    assert_eq!(seen_errors.len(), 1);
    assert!(seen_errors[0].contains("upstream unavailable"));
}
