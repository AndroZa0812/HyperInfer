use reqwest::Client;
use serde::{Deserialize, Serialize};
use hyperinfer_core::{ChatRequest, ChatResponse, HyperInferError};

pub struct HttpCaller {
    client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiResponse {
    pub id: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
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
        let url = format!("https://api.openai.com/v1/chat/completions");
        
        let body = serde_json::json!({
            "model": model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens,
        });

        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        let data: OpenAiResponse = response.json().await?;
        
        Ok(ChatResponse {
            id: data.id,
            model: model.to_string(),
            choices: data.choices.into_iter().map(|c| c.message.content).collect(),
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
        
        let system = request.messages.iter()
            .find(|m| m.role == hyperinfer_core::types::MessageRole::System)
            .map(|m| m.content.clone());
            
        let messages: Vec<_> = request.messages.iter()
            .filter(|m| m.role != hyperinfer_core::types::MessageRole::System)
            .map(|m| serde_json::json!({
                "role": match m.role {
                    hyperinfer_core::types::MessageRole::User => "user",
                    hyperinfer_core::types::MessageRole::Assistant => "assistant",
                    _ => "user",
                },
                "content": m.content
            }))
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

        let response = self.client
            .post(url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

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
        
        let content = data.content.into_iter()
            .filter_map(|b| b.text)
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ChatResponse {
            id: data.id,
            model: model.to_string(),
            choices: vec![content],
            usage: hyperinfer_core::types::Usage {
                input_tokens: data.usage.input_tokens,
                output_tokens: data.usage.output_tokens,
            },
        })
    }
}
