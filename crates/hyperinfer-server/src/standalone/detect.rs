use std::collections::HashMap;
use std::time::Duration;

use hyperinfer_core::types::CreateDeploymentRequest;

#[derive(Debug, Clone)]
pub struct DetectedProvider {
    pub name: String,
    pub provider: String,
    pub models: Vec<String>,
    pub base_url: Option<String>,
    pub api_key_ref: String,
}

pub fn auto_detect_providers_from_env(env: &HashMap<String, String>) -> Vec<DetectedProvider> {
    let mut providers = Vec::new();

    if let Some(key) = env.get("OPENAI_API_KEY") {
        if !key.is_empty() {
            providers.push(DetectedProvider {
                name: "openai".to_string(),
                provider: "openai".to_string(),
                models: vec!["gpt-4o-mini".to_string(), "gpt-4o".to_string()],
                base_url: None,
                api_key_ref: "$OPENAI_API_KEY".to_string(),
            });
        }
    }

    if let Some(key) = env.get("ANTHROPIC_API_KEY") {
        if !key.is_empty() {
            providers.push(DetectedProvider {
                name: "anthropic".to_string(),
                provider: "anthropic".to_string(),
                models: vec![
                    "claude-sonnet-4-20250514".to_string(),
                    "claude-3-5-sonnet-latest".to_string(),
                ],
                base_url: None,
                api_key_ref: "$ANTHROPIC_API_KEY".to_string(),
            });
        }
    }

    if let Some(key) = env.get("GEMINI_API_KEY") {
        if !key.is_empty() {
            providers.push(DetectedProvider {
                name: "gemini".to_string(),
                provider: "gemini".to_string(),
                models: vec![
                    "gemini-2.5-flash".to_string(),
                    "gemini-2.0-flash".to_string(),
                ],
                base_url: None,
                api_key_ref: "$GEMINI_API_KEY".to_string(),
            });
        }
    }

    providers
}

pub async fn probe_ollama(host: &str) -> Option<DetectedProvider> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(300))
        .build()
        .ok()?;

    let url = format!("{}/api/tags", host.trim_end_matches('/'));
    let resp = client.get(&url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }

    let body: serde_json::Value = resp.json().await.ok()?;
    let models = body
        .get("models")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let models = if models.is_empty() {
        vec!["llama3.2".to_string()]
    } else {
        models
    };

    Some(DetectedProvider {
        name: "ollama".to_string(),
        provider: "openai".to_string(),
        models,
        base_url: Some(format!("{}/v1", host.trim_end_matches('/'))),
        api_key_ref: "ollama".to_string(),
    })
}

pub async fn auto_detect_providers(probe_local_ollama: bool) -> Vec<DetectedProvider> {
    let env: HashMap<String, String> = std::env::vars().collect();
    let mut providers = auto_detect_providers_from_env(&env);

    // Enrich env-detected providers with live model lists from each provider's native API
    // (short 2s timeout so standalone startup stays snappy even when offline).
    for detected in &mut providers {
        let env_var = detected.api_key_ref.trim_start_matches('$');
        let Some(api_key) = env.get(env_var).filter(|k| !k.is_empty()) else {
            continue;
        };
        let live_models: Option<Vec<String>> =
            tokio::time::timeout(Duration::from_secs(2), async {
                use hyperinfer_providers::{
                    anthropic::AnthropicProvider, gemini::GeminiProvider, openai::OpenAiProvider,
                    LlmProvider,
                };
                let res = match detected.provider.as_str() {
                    "openai" => OpenAiProvider::new().ok()?.list_models(api_key).await.ok(),
                    "anthropic" => AnthropicProvider::new()
                        .ok()?
                        .list_models(api_key)
                        .await
                        .ok(),
                    "gemini" => GeminiProvider::new().ok()?.list_models(api_key).await.ok(),
                    _ => None,
                }?;
                let ids: Vec<String> = res.into_iter().take(20).map(|m| m.id).collect();
                if ids.is_empty() {
                    None
                } else {
                    Some(ids)
                }
            })
            .await
            .ok()
            .flatten();

        if let Some(ids) = live_models {
            detected.models = ids;
        }
    }

    if probe_local_ollama {
        let host = env
            .get("OLLAMA_HOST")
            .cloned()
            .unwrap_or_else(|| "http://localhost:11434".to_string());
        if let Some(ollama) = probe_ollama(&host).await {
            providers.push(ollama);
        }
    }

    providers
}

fn default_base_url(provider: &str) -> String {
    match provider {
        "anthropic" => "https://api.anthropic.com".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com".to_string(),
        "antigravity" => "https://cloudcode-pa.googleapis.com".to_string(),
        _ => "https://api.openai.com".to_string(),
    }
}

pub fn detected_to_deployments(providers: &[DetectedProvider]) -> Vec<CreateDeploymentRequest> {
    let mut deployments = Vec::new();

    for provider in providers {
        for model in &provider.models {
            deployments.push(CreateDeploymentRequest {
                name: format!("{}-{}", provider.name, model),
                provider: provider.provider.clone(),
                model: model.clone(),
                api_key_ref: Some(provider.api_key_ref.clone()),
                base_url: provider
                    .base_url
                    .clone()
                    .unwrap_or_else(|| default_base_url(&provider.provider)),
                is_active: true,
                weight: 1,
                priority: 0,
                max_tpm: None,
                max_rpm: None,
                cost_per_1k_input_tokens: None,
                cost_per_1k_output_tokens: None,
                metadata: None,
                sort_order: None,
            });
        }
    }

    deployments
}
