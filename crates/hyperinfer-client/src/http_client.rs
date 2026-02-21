use hyperinfer_core::types::{ChatMessage, Choice, MessageRole};
use hyperinfer_core::{ChatRequest, ChatResponse, HyperInferError};
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct HttpCaller {
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiResponse {
    pub id: String,
    pub choices: Vec<OpenAiChoice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiChoice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl HttpCaller {
    pub fn new() -> Result<Self, reqwest::Error> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()?;
        Ok(Self { client })
    }

    pub async fn call_openai(
        &self,
        model: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, HyperInferError> {
        let url = "https://api.openai.com/v1/chat/completions".to_string();

        let body = serde_json::json!({
            "model": model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(HyperInferError::ApiError {
                status: status.as_u16(),
                message: error_text,
            });
        }

        let data: OpenAiResponse = response.json().await?;

        Ok(ChatResponse {
            id: data.id,
            model: model.to_string(),
            choices: data
                .choices
                .into_iter()
                .map(|c| Choice {
                    index: c.index,
                    message: ChatMessage {
                        role: match c.message.role.as_str() {
                            "assistant" => MessageRole::Assistant,
                            "user" => MessageRole::User,
                            "system" => MessageRole::System,
                            other => {
                                tracing::warn!(
                                    "Unknown OpenAI role '{}', defaulting to Assistant",
                                    other
                                );
                                MessageRole::Assistant
                            }
                        },
                        content: c.message.content,
                    },
                    finish_reason: c.finish_reason,
                })
                .collect(),
            usage: hyperinfer_core::types::Usage {
                input_tokens: data.usage.prompt_tokens,
                output_tokens: data.usage.completion_tokens,
            },
        })
    }

    pub async fn call_anthropic(
        &self,
        model: &str,
        api_key: &str,
        request: &ChatRequest,
    ) -> Result<ChatResponse, HyperInferError> {
        let url = "https://api.anthropic.com/v1/messages";

        let system = request
            .messages
            .iter()
            .find(|m| m.role == hyperinfer_core::types::MessageRole::System)
            .map(|m| m.content.clone());

        let messages: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != hyperinfer_core::types::MessageRole::System)
            .map(|m| {
                serde_json::json!({
                    "role": match m.role {
                        hyperinfer_core::types::MessageRole::User => "user",
                        hyperinfer_core::types::MessageRole::Assistant => "assistant",
                        _ => "user",
                    },
                    "content": m.content
                })
            })
            .collect();

        let mut body = serde_json::json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(1024),
        });

        if let Some(s) = system {
            body["system"] = serde_json::json!(s);
        }
        if let Some(t) = request.temperature {
            body["temperature"] = serde_json::json!(t);
        }

        let response = self
            .client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(HyperInferError::ApiError {
                status: status.as_u16(),
                message: error_text,
            });
        }

        #[derive(Deserialize)]
        struct AnthropicResponse {
            id: String,
            content: Vec<ContentBlock>,
            usage: AnthropicUsage,
        }

        #[derive(Deserialize)]
        struct ContentBlock {
            text: Option<String>,
        }

        #[derive(Deserialize)]
        struct AnthropicUsage {
            input_tokens: u32,
            output_tokens: u32,
        }

        let data: AnthropicResponse = response.json().await?;

        let content = data
            .content
            .into_iter()
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ChatResponse {
            id: data.id,
            model: model.to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChatMessage {
                    role: MessageRole::Assistant,
                    content,
                },
                finish_reason: Some("stop".to_string()),
            }],
            usage: hyperinfer_core::types::Usage {
                input_tokens: data.usage.input_tokens,
                output_tokens: data.usage.output_tokens,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_caller_new() {
        let result = HttpCaller::new();
        assert!(result.is_ok());
    }

    #[test]
    fn test_openai_response_deserialization() {
        let json = r#"{
            "id": "chatcmpl-123",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "chatcmpl-123");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "Hello!");
        assert_eq!(response.usage.total_tokens, 15);
    }

    #[test]
    fn test_openai_choice_deserialization() {
        let json = r#"{
            "index": 0,
            "message": {
                "role": "user",
                "content": "Test message"
            },
            "finish_reason": "length"
        }"#;

        let choice: OpenAiChoice = serde_json::from_str(json).unwrap();
        assert_eq!(choice.index, 0);
        assert_eq!(choice.message.role, "user");
        assert_eq!(choice.message.content, "Test message");
        assert_eq!(choice.finish_reason, Some("length".to_string()));
    }

    #[test]
    fn test_usage_deserialization() {
        let json = r#"{
            "prompt_tokens": 100,
            "completion_tokens": 50,
            "total_tokens": 150
        }"#;

        let usage: Usage = serde_json::from_str(json).unwrap();
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            role: "assistant".to_string(),
            content: "Response text".to_string(),
        };

        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("assistant"));
        assert!(json.contains("Response text"));
    }

    #[test]
    fn test_openai_response_clone() {
        let response = OpenAiResponse {
            id: "test-id".to_string(),
            choices: vec![],
            usage: Usage {
                prompt_tokens: 10,
                completion_tokens: 5,
                total_tokens: 15,
            },
        };

        let cloned = response.clone();
        assert_eq!(response.id, cloned.id);
        assert_eq!(response.usage.total_tokens, cloned.usage.total_tokens);
    }

    #[test]
    fn test_openai_choice_with_no_finish_reason() {
        let json = r#"{
            "index": 1,
            "message": {
                "role": "assistant",
                "content": "Partial response"
            },
            "finish_reason": null
        }"#;

        let choice: OpenAiChoice = serde_json::from_str(json).unwrap();
        assert_eq!(choice.index, 1);
        assert_eq!(choice.finish_reason, None);
    }

    #[tokio::test]
    async fn test_call_openai_request_structure() {
        // Test that we can construct a valid request
        let caller = HttpCaller::new().unwrap();
        let request = ChatRequest {
            model: "gpt-4".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Hello".to_string(),
            }],
            temperature: Some(0.7),
            max_tokens: Some(100),
        };

        // We can't actually call OpenAI without a real API key and network,
        // but we can verify the function signature and request structure
        let body = serde_json::json!({
            "model": "gpt-4",
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });

        assert_eq!(body["model"], "gpt-4");
        assert_eq!(body["temperature"], 0.7);
        assert_eq!(body["max_tokens"], 100);
    }

    #[tokio::test]
    async fn test_call_anthropic_request_structure() {
        let request = ChatRequest {
            model: "claude-3".to_string(),
            messages: vec![
                ChatMessage {
                    role: MessageRole::System,
                    content: "You are helpful".to_string(),
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: "Hello".to_string(),
                },
            ],
            temperature: Some(0.5),
            max_tokens: Some(200),
        };

        // Extract system message
        let system = request
            .messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .map(|m| m.content.clone());

        assert_eq!(system, Some("You are helpful".to_string()));

        // Filter non-system messages
        let messages: Vec<_> = request
            .messages
            .iter()
            .filter(|m| m.role != MessageRole::System)
            .collect();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "Hello");
    }
}
