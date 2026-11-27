#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, Choice, MessageRole};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    fn sample_request(content: &str) -> ChatRequest {
        ChatRequest {
            model: "test".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: content.to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                content_parts: None,
            }],
            temperature: None,
            max_tokens: None,
            stream: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            prompt_caching: None,
            stop: None,
        }
    }

    fn sample_response(content: &str) -> ChatResponse {
        ChatResponse {
            id: "test".to_string(),
            model: "test".to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: MessageRole::Assistant,
                    content: content.to_string(),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                    content_parts: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Default::default(),
        }
    }

    async fn spawn_mock_http_server(status_line: &'static str, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });
        format!("http://{}", addr)
    }

    #[test]
    fn test_api_key_and_bearer_redaction() {
        let plugin = RedactionPlugin::with_defaults();

        let input = "My key is sk-ant-abc123def456ghi789 and token Bearer eyJhbGciOiJIUzI1NiJ9.test";
        let output = plugin.redact_str(input);

        assert!(output.contains("[REDACTED_API_KEY]"));
        assert!(output.contains("Bearer [REDACTED_TOKEN]"));
        assert!(!output.contains("sk-ant-abc123def456ghi789"));
        assert!(!output.contains("eyJhbGciOiJIUzI1NiJ9.test"));
    }

    #[test]
    fn test_email_ssn_custom_redaction() {
        let config = RedactionConfig {
            custom_patterns: vec![(
                r"\b\d{4}-\d{4}-\d{4}-\d{4}\b".to_string(),
                "[REDACTED_CC]".to_string(),
            )],
            ..Default::default()
        };
        let plugin = RedactionPlugin::new(config);

        let input = "Contact user@example.com, SSN 123-45-6789, card 4111-1111-1111-1111";
        let output = plugin.redact_str(input);

        assert!(output.contains("[REDACTED_EMAIL]"));
        assert!(output.contains("[REDACTED_SSN]"));
        assert!(output.contains("[REDACTED_CC]"));
        assert!(!output.contains("user@example.com"));
        assert!(!output.contains("123-45-6789"));
        assert!(!output.contains("4111-1111-1111-1111"));
    }

    #[tokio::test]
    async fn test_plugin_request_response_scrubbing_one_way_opt_out() {
        let plugin = RedactionPlugin::with_defaults();
        let mut req = sample_request("My key is sk-ant-abc123def456ghi789");

        let mut ctx = PluginContext::default();
        let action = plugin.pre_request(&mut ctx, &mut req).await;
        assert!(matches!(action, PluginAction::Continue));
        assert!(req.messages[0].content.contains("[REDACTED_API_KEY]"));
        assert!(!req.messages[0].content.contains("sk-ant-abc123def456ghi789"));
        assert!(!ctx.metadata.contains_key(REDACTION_VAULT_METADATA_KEY));

        let mut resp = sample_response("Your key sk-ant-abc123def456ghi789 is valid");
        plugin.post_response(&mut ctx, &mut resp).await;
        assert!(resp.choices[0].message.content.contains("[REDACTED_API_KEY]"));
        assert!(!resp.choices[0]
            .message
            .content
            .contains("sk-ant-abc123def456ghi789"));
    }

    #[tokio::test]
    async fn test_reversible_indexed_roundtrip_and_deduplication() {
        let plugin = RedactionPlugin::new(RedactionConfig {
            reversible: true,
            placeholder_format: PlaceholderFormat::Indexed,
            ..Default::default()
        });

        let secret = "sk-ant-abc123def456ghi789";
        let mut req = sample_request(&format!(
            "Use {secret} first, and repeat {secret} for alice@example.com"
        ));
        let mut ctx = PluginContext::default();

        let action = plugin.pre_request(&mut ctx, &mut req).await;
        assert!(matches!(action, PluginAction::Continue));
        assert_eq!(
            req.messages[0].content,
            "Use [REDACTED_API_KEY_1] first, and repeat [REDACTED_API_KEY_1] for [REDACTED_EMAIL_2]"
        );
        assert!(ctx.metadata.contains_key(REDACTION_VAULT_METADATA_KEY));

        let mut resp =
            sample_response("Configured [REDACTED_API_KEY_1] and notified [REDACTED_EMAIL_2].");
        plugin.post_response(&mut ctx, &mut resp).await;

        assert_eq!(
            resp.choices[0].message.content,
            format!("Configured {secret} and notified alice@example.com.")
        );
        // Vault is removed from context after restoration.
        assert!(!ctx.metadata.contains_key(REDACTION_VAULT_METADATA_KEY));
    }

    #[tokio::test]
    async fn test_reversible_uuid_roundtrip() {
        let plugin = RedactionPlugin::new(RedactionConfig {
            reversible: true,
            placeholder_format: PlaceholderFormat::Uuid,
            ..Default::default()
        });

        let mut req = sample_request("Email bob@example.com");
        let mut ctx = PluginContext::default();
        plugin.pre_request(&mut ctx, &mut req).await;

        let redacted = req.messages[0].content.clone();
        assert!(redacted.starts_with("Email [REDACTED_EMAIL_"));
        assert!(redacted.ends_with(']'));
        assert!(!redacted.contains("bob@example.com"));

        let placeholder = redacted.trim_start_matches("Email ");
        let mut resp = sample_response(&format!("Sent to {placeholder}"));
        plugin.post_response(&mut ctx, &mut resp).await;
        assert_eq!(resp.choices[0].message.content, "Sent to bob@example.com");
    }

    #[test]
    fn test_stream_rehydrator_across_split_chunks() {
        let mut vault = RedactionVault::new();
        let placeholder = vault.tokenize("API_KEY", "sk-secret-9999999999", PlaceholderFormat::Indexed);
        assert_eq!(placeholder, "[REDACTED_API_KEY_1]");

        let mut rehydrator = StreamRehydrator::new(vault);
        let chunks = ["Key is ", "[RED", "ACTED_API_", "KEY_1]", " works!"];
        let mut assembled = String::new();
        for c in chunks {
            assembled.push_str(&rehydrator.push_chunk(c));
        }
        assembled.push_str(&rehydrator.flush());
        assert_eq!(assembled, "Key is sk-secret-9999999999 works!");
    }

    #[tokio::test]
    async fn test_presidio_smart_redaction_and_reversible_composition() {
        let presidio_body = serde_json::json!([
            {"start": 8, "end": 21, "entity_type": "PERSON", "score": 0.92},
            {"start": 25, "end": 31, "entity_type": "LOCATION", "score": 0.85},
            {"start": 8, "end": 13, "entity_type": "PERSON", "score": 0.40}
        ])
        .to_string();
        let base_url = spawn_mock_http_server("200 OK", presidio_body).await;

        let plugin = RedactionPlugin::new(RedactionConfig {
            reversible: true,
            placeholder_format: PlaceholderFormat::Indexed,
            smart: SmartRedactionConfig {
                enabled: true,
                backend: SmartRedactionBackend::Presidio,
                presidio_url: base_url,
                presidio_score_threshold: 0.5,
                timeout_ms: 2000,
                fail_mode: SmartRedactionFailMode::FailClosed,
                ..Default::default()
            },
            ..Default::default()
        });

        let mut req = sample_request("Contact Alice Johnson in Berlin");
        let mut ctx = PluginContext::default();
        let action = plugin.pre_request(&mut ctx, &mut req).await;
        assert!(matches!(action, PluginAction::Continue));
        assert_eq!(
            req.messages[0].content,
            "Contact [REDACTED_PERSON_1] in [REDACTED_LOCATION_2]"
        );

        let mut resp = sample_response("Reached [REDACTED_PERSON_1] at [REDACTED_LOCATION_2]");
        plugin.post_response(&mut ctx, &mut resp).await;
        assert_eq!(
            resp.choices[0].message.content,
            "Reached Alice Johnson at Berlin"
        );
    }

    #[tokio::test]
    async fn test_llm_smart_redaction() {
        let llm_body = serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"spans\": [{\"text\": \"Project Apollo\", \"category\": \"internal_names\"}]}"
                }
            }]
        })
        .to_string();
        let base_url = spawn_mock_http_server("200 OK", llm_body).await;

        let plugin = RedactionPlugin::new(RedactionConfig {
            reversible: false,
            smart: SmartRedactionConfig {
                enabled: true,
                backend: SmartRedactionBackend::Llm,
                llm_base_url: base_url,
                llm_model: "qwen2.5:3b".to_string(),
                timeout_ms: 2000,
                fail_mode: SmartRedactionFailMode::FailClosed,
                ..Default::default()
            },
            ..Default::default()
        });

        let output = plugin
            .redact_str_async("Launching Project Apollo tomorrow", None)
            .await
            .unwrap();
        assert_eq!(output, "Launching [REDACTED_INTERNAL_NAMES] tomorrow");
    }

    #[tokio::test]
    async fn test_smart_redaction_fail_closed_vs_fallback() {
        let dead_url = spawn_mock_http_server("500 Internal Server Error", "err".into()).await;
        let plugin_closed = RedactionPlugin::new(RedactionConfig {
            smart: SmartRedactionConfig {
                enabled: true,
                backend: SmartRedactionBackend::Presidio,
                presidio_url: dead_url,
                timeout_ms: 1000,
                fail_mode: SmartRedactionFailMode::FailClosed,
                ..Default::default()
            },
            ..Default::default()
        });

        let mut req = sample_request("Email alice@example.com");
        let mut ctx = PluginContext::default();
        let action = plugin_closed.pre_request(&mut ctx, &mut req).await;
        assert!(matches!(
            action,
            PluginAction::ShortCircuit {
                status_code: 503,
                ..
            }
        ));

        let dead_url2 = spawn_mock_http_server("500 Internal Server Error", "err".into()).await;
        let plugin_fallback = RedactionPlugin::new(RedactionConfig {
            smart: SmartRedactionConfig {
                enabled: true,
                backend: SmartRedactionBackend::Presidio,
                presidio_url: dead_url2,
                timeout_ms: 1000,
                fail_mode: SmartRedactionFailMode::FallbackRegex,
                ..Default::default()
            },
            ..Default::default()
        });

        let mut req2 = sample_request("Email alice@example.com");
        let mut ctx2 = PluginContext::default();
        let action2 = plugin_fallback.pre_request(&mut ctx2, &mut req2).await;
        assert!(matches!(action2, PluginAction::Continue));
        assert_eq!(req2.messages[0].content, "Email [REDACTED_EMAIL]");
    }

    #[tokio::test]
    async fn test_skip_redaction_recursion_guard() {
        let plugin = RedactionPlugin::with_defaults();
        let mut req = sample_request("Secret sk-ant-abc123def456ghi789");
        let mut ctx = PluginContext::default();
        ctx.metadata.insert(
            SKIP_REDACTION_METADATA_KEY.to_string(),
            serde_json::Value::Bool(true),
        );

        let action = plugin.pre_request(&mut ctx, &mut req).await;
        assert!(matches!(action, PluginAction::Continue));
        assert_eq!(req.messages[0].content, "Secret sk-ant-abc123def456ghi789");
    }

    #[tokio::test]
    async fn test_reversible_without_vault_preserves_response_redaction() {
        let plugin = RedactionPlugin::new(RedactionConfig {
            reversible: true,
            redact_requests: false,
            redact_responses: true,
            ..Default::default()
        });

        let mut req = sample_request("Hello");
        let mut ctx = PluginContext::default();
        plugin.pre_request(&mut ctx, &mut req).await;
        assert!(!ctx.metadata.contains_key(REDACTION_VAULT_METADATA_KEY));

        let mut resp = sample_response("Leaked key sk-ant-abc123def456ghi789");
        plugin.post_response(&mut ctx, &mut resp).await;
        assert_eq!(
            resp.choices[0].message.content,
            "Leaked key [REDACTED_API_KEY]"
        );
    }

    #[test]
    fn test_stream_rehydrator_caps_unclosed_placeholder_buffer() {
        let mut vault = RedactionVault::new();
        let placeholder = vault.tokenize("EMAIL", "alice@example.com", PlaceholderFormat::Indexed);
        assert_eq!(placeholder, "[REDACTED_EMAIL_1]");

        let mut rehydrator = StreamRehydrator::new(vault);
        // Unclosed "[REDACTED_" longer than max placeholder length must be emitted immediately
        let unclosed = "[REDACTED_not_a_real_placeholder_with_extra_text";
        let emitted = rehydrator.push_chunk(unclosed);
        assert_eq!(emitted, unclosed);
        assert_eq!(rehydrator.flush(), "");
    }
}
