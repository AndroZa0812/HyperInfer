#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, Choice, MessageRole};

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
            custom_patterns: vec![(r"\b\d{4}-\d{4}-\d{4}-\d{4}\b".to_string(), "[REDACTED_CC]".to_string())],
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
    async fn test_plugin_request_response_scrubbing() {
        let plugin = RedactionPlugin::with_defaults();

        let mut req = ChatRequest {
            model: "test".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "My key is sk-ant-abc123def456ghi789".to_string(),
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
        };

        let mut ctx = PluginContext::default();
        let action = plugin.pre_request(&mut ctx, &mut req).await;
        assert!(matches!(action, PluginAction::Continue));
        assert!(req.messages[0].content.contains("[REDACTED_API_KEY]"));
        assert!(!req.messages[0].content.contains("sk-ant-abc123def456ghi789"));

        let mut resp = ChatResponse {
            id: "test".to_string(),
            model: "test".to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: MessageRole::Assistant,
                    content: "Your key sk-ant-abc123def456ghi789 is valid".to_string(),
                    name: None,
                    tool_calls: None,
                    tool_call_id: None,
                    content_parts: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: Default::default(),
        };

        plugin.post_response(&mut ctx, &mut resp).await;
        assert!(resp.choices[0].message.content.contains("[REDACTED_API_KEY]"));
        assert!(!resp.choices[0].message.content.contains("sk-ant-abc123def456ghi789"));
    }
}
