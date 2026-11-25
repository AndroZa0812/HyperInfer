//! Multi-SDK proxy routes: Anthropic Messages, OpenAI Responses, Gemini generateContent.
//!
//! All three routes share a single `dispatch_sdk_chat` helper that handles auth,
//! quota enforcement, deployment selection, streaming/non-streaming forwarding,
//! and usage recording. Protocol-specific translation is delegated to the
//! `protocols` modules.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::StreamExt;
use hyperinfer_core::{ChatRequest, ConfigStore, Database};
use tracing::warn;

use crate::protocols::{anthropic, gemini, responses};
use crate::proxy;
use crate::routes::routing::ProxyError;
use crate::state::AppState;

/// Target SDK format for response translation.
#[derive(Debug, Clone, Copy)]
enum SdkFormat {
    Anthropic,
    Responses,
    Gemini,
}

/// Shared dispatch for all multi-SDK chat routes.
async fn dispatch_sdk_chat<D: Database + Clone + Send + Sync + 'static, C: ConfigStore>(
    state: AppState<D, C>,
    api_key: &str,
    mut request: ChatRequest,
    request_model: String,
    format: SdkFormat,
) -> Result<Response, ProxyError> {
    let start = std::time::Instant::now();

    // 1. Validate API key
    let auth = match proxy::validate_api_key(&state.db, api_key).await {
        Ok(a) => a,
        Err(status) => {
            return Err(ProxyError {
                error: "Invalid or missing API key".to_string(),
                code: status,
            })
        }
    };

    // 2. Enforce team RPM/TPM quotas
    let estimated_tokens: u64 = (request
        .messages
        .iter()
        .map(|m| m.content.len())
        .sum::<usize>()
        / 4)
    .max(1) as u64;
    let mut team_tpm_limit: Option<u64> = None;

    if let Ok(Some(quota)) = state.db.get_quota(&auth.team_id).await {
        if let Some(ref limiter) = state.rate_limiter {
            if quota.rpm_limit > 0 {
                match limiter
                    .check_rpm(&auth.team_id, quota.rpm_limit as u64)
                    .await
                {
                    Ok((false, _)) => {
                        return Err(ProxyError {
                            error: "Team RPM quota exceeded".to_string(),
                            code: 429,
                        });
                    }
                    Err(e) => warn!(team_id = %auth.team_id, error = %e, "RPM quota check failed"),
                    Ok((true, _)) => {}
                }
            }
            if quota.tpm_limit > 0 {
                let tpm_limit = quota.tpm_limit as u64;
                team_tpm_limit = Some(tpm_limit);
                match limiter
                    .check_tpm(&auth.team_id, tpm_limit, estimated_tokens)
                    .await
                {
                    Ok(false) => {
                        return Err(ProxyError {
                            error: "Team TPM quota exceeded".to_string(),
                            code: 429,
                        });
                    }
                    Err(e) => warn!(team_id = %auth.team_id, error = %e, "TPM quota check failed"),
                    Ok(true) => {}
                }
            }
        }
    }

    // 3. Load active deployments
    let deployments = state
        .db
        .list_deployments(&request.model, Some(true))
        .await
        .map_err(|e| ProxyError {
            error: format!("Failed to load deployments: {}", e),
            code: 500,
        })?;

    if deployments.is_empty() {
        return Err(ProxyError {
            error: format!("No active deployments found for model '{}'", request.model),
            code: 404,
        });
    }

    // 4. Select deployment
    let proxy_router = match &state.proxy_router {
        Some(r) => Arc::clone(r),
        None => Arc::new(proxy::ProxyRouter::new_in_memory().await),
    };

    let model_aliases = state.config.read().await.model_aliases.clone();
    let selected = proxy_router
        .select_deployment(
            &state.db,
            &request,
            &deployments,
            &model_aliases,
            Some(&auth),
        )
        .await
        .map_err(|e| ProxyError {
            error: format!("Routing failed: {}", e),
            code: 503,
        })?;

    selected.mutators.apply_to_request(&mut request);
    let deployment_id = selected.deployment.id.clone();

    // 5. Streaming path
    if request.stream == Some(true) {
        let stream = match proxy::forward_chat_stream(
            proxy_router.providers(),
            &request,
            &selected.base_url,
            &selected.api_key,
            &selected.provider,
        )
        .await
        {
            Ok(s) => s,
            Err(code) => {
                proxy_router.record_failure(&deployment_id).await;
                return Err(ProxyError {
                    error: "Upstream streaming request failed".to_string(),
                    code,
                });
            }
        };

        let db = state.db.clone();
        let router_clone = Arc::clone(&proxy_router);
        let team_id = auth.team_id.clone();
        let api_key_id = auth.api_key_id.clone();
        let model_name = request.model.clone();

        let sse_stream = async_stream::stream! {
            let mut input_tokens: u32 = 0;
            let mut output_tokens: u32 = 0;
            let mut stream = stream;
            let mut is_first = true;
            let mut stream_failed = false;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(chunk) => {
                        if let Some(ref usage) = chunk.usage {
                            input_tokens = usage.input_tokens;
                            output_tokens = usage.output_tokens;
                        }
                        let events = match format {
                            SdkFormat::Anthropic => anthropic::chunk_to_sse_events(&chunk, is_first),
                            SdkFormat::Responses => responses::chunk_to_sse_events(&chunk, is_first),
                            SdkFormat::Gemini => vec![("message".to_string(), gemini::chunk_to_sse_json(&chunk))],
                        };
                        for (event, data) in events {
                            yield Ok::<axum::response::sse::Event, std::convert::Infallible>(
                                axum::response::sse::Event::default().event(event).data(data),
                            );
                        }
                        is_first = false;
                    }
                    Err(e) => {
                        warn!(error = %e, "Error in upstream SSE chunk");
                        router_clone.record_failure(&deployment_id).await;
                        stream_failed = true;
                        break;
                    }
                }
            }

            if stream_failed {
                yield Ok::<axum::response::sse::Event, std::convert::Infallible>(
                    axum::response::sse::Event::default()
                        .event("error")
                        .data("Upstream streaming request failed"),
                );
                return;
            }

            let elapsed_ms = start.elapsed().as_millis() as i64;
            let total_tokens = (input_tokens + output_tokens) as u64;
            router_clone
                .record_success(&deployment_id, elapsed_ms as f64, total_tokens)
                .await;
            let _ = db
                .record_usage(
                    &team_id,
                    &api_key_id,
                    &model_name,
                    input_tokens as i32,
                    output_tokens as i32,
                    elapsed_ms,
                )
                .await;
        };

        return Ok(axum::response::sse::Sse::new(sse_stream)
            .keep_alive(axum::response::sse::KeepAlive::default())
            .into_response());
    }

    // 6. Non-streaming path
    let response = match proxy::forward_chat(
        proxy_router.providers(),
        &request,
        &selected.base_url,
        &selected.api_key,
        &selected.provider,
    )
    .await
    {
        Ok(resp) => resp,
        Err(code) => {
            proxy_router.record_failure(&deployment_id).await;
            return Err(ProxyError {
                error: "Upstream request failed".to_string(),
                code,
            });
        }
    };

    let elapsed_ms = start.elapsed().as_millis() as i64;
    let total_tokens = (response.usage.input_tokens + response.usage.output_tokens) as u64;
    proxy_router
        .record_success(&deployment_id, elapsed_ms as f64, total_tokens)
        .await;

    if let (Some(ref limiter), Some(tpm_limit)) = (&state.rate_limiter, team_tpm_limit) {
        if total_tokens > estimated_tokens {
            if let Err(e) = limiter
                .check_tpm(&auth.team_id, tpm_limit, total_tokens - estimated_tokens)
                .await
            {
                warn!(team_id = %auth.team_id, error = %e, "Post-response TPM accounting failed");
            }
        }
    }

    if let Err(e) = state
        .db
        .record_usage(
            &auth.team_id,
            &auth.api_key_id,
            &request.model,
            response.usage.input_tokens as i32,
            response.usage.output_tokens as i32,
            elapsed_ms,
        )
        .await
    {
        warn!(error = ?e, "Failed to record proxy usage log");
    }

    let body = match format {
        SdkFormat::Anthropic => anthropic::from_chat_response(&response, &request_model),
        SdkFormat::Responses => responses::from_chat_response(&response, &request_model),
        SdkFormat::Gemini => gemini::from_chat_response(&response),
    };

    Ok(Json(body).into_response())
}

/// Anthropic Messages API: POST /v1/messages
pub async fn anthropic_messages_handler<
    D: Database + Clone + Send + Sync + 'static,
    C: ConfigStore,
>(
    State(state): State<AppState<D, C>>,
    headers: HeaderMap,
    Json(req): Json<anthropic::AnthropicMessagesRequest>,
) -> Result<Response, ProxyError> {
    let api_key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        })
        .unwrap_or("");

    let request = anthropic::to_chat_request(req);
    let model = request.model.clone();
    dispatch_sdk_chat(state, api_key, request, model, SdkFormat::Anthropic).await
}

/// OpenAI Responses API: POST /v1/responses
pub async fn openai_responses_handler<
    D: Database + Clone + Send + Sync + 'static,
    C: ConfigStore,
>(
    State(state): State<AppState<D, C>>,
    headers: HeaderMap,
    Json(req): Json<responses::ResponsesRequest>,
) -> Result<Response, ProxyError> {
    let api_key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let request = responses::to_chat_request(req);
    let model = request.model.clone();
    dispatch_sdk_chat(state, api_key, request, model, SdkFormat::Responses).await
}

/// Gemini generateContent API: POST /v1beta/models/{model}:{action}
pub async fn gemini_generate_content_handler<
    D: Database + Clone + Send + Sync + 'static,
    C: ConfigStore,
>(
    State(state): State<AppState<D, C>>,
    headers: HeaderMap,
    Path(model_action): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    Json(req): Json<gemini::GeminiGenerateRequest>,
) -> Result<Response, ProxyError> {
    let (model, action) = match model_action.split_once(':') {
        Some((m, a)) => (m.to_string(), a.to_string()),
        None => {
            return Err(ProxyError {
                error: "Invalid model path: expected 'model:action'".to_string(),
                code: 400,
            })
        }
    };

    let stream = match action.as_str() {
        "generateContent" => None,
        "streamGenerateContent" => Some(true),
        _ => {
            return Err(ProxyError {
                error: format!("Unknown Gemini action: {}", action),
                code: 400,
            })
        }
    };

    let api_key = headers
        .get("x-goog-api-key")
        .and_then(|v| v.to_str().ok())
        .or_else(|| query.get("key").map(String::as_str))
        .or_else(|| {
            headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
        })
        .unwrap_or("");

    let mut request = gemini::to_chat_request(&model, req);
    request.stream = stream;

    dispatch_sdk_chat(state, api_key, request, model, SdkFormat::Gemini).await
}
