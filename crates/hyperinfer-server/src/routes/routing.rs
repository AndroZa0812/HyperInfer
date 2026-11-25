//! Routing configuration, config sync, and OpenAI-compatible chat completions proxy routes.

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::{
        sse::{Event as SseEvent, KeepAlive, Sse},
        IntoResponse, Response,
    },
};
use futures::StreamExt;
use hyperinfer_core::{ConfigStore, Database, UpdateRoutingConfigRequest};
use serde::Serialize;
use std::sync::Arc;

use crate::proxy;
use crate::state::AppState;

pub async fn config_sync<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    Json(config.clone())
}

pub async fn get_routing_config_handler<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    match state.db.get_routing_config().await {
        Ok(Some(config)) => Json(config).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Routing config not found").into_response(),
        Err(e) => {
            tracing::error!("Failed to get routing config: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to get routing config",
            )
                .into_response()
        }
    }
}

pub async fn update_routing_config_handler<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<UpdateRoutingConfigRequest>,
) -> impl IntoResponse {
    match state.db.update_routing_config(req).await {
        Ok(config) => Json(config).into_response(),
        Err(e) => {
            tracing::error!("Failed to update routing config: {:?}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to update routing config",
            )
                .into_response()
        }
    }
}

pub async fn get_routing_health<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    let deployments = state.db.list_deployments("", Some(true)).await;
    let deployment_count = match deployments {
        Ok(d) => d.len(),
        Err(_) => 0,
    };

    let config = state.db.get_routing_config().await;
    let strategy = match config {
        Ok(Some(c)) => c.strategy,
        Ok(None) => "weighted-shuffle".to_string(),
        Err(_) => "unknown".to_string(),
    };

    Json(serde_json::json!({
        "active_deployments": deployment_count,
        "strategy": strategy,
        "status": "healthy",
    }))
    .into_response()
}

#[derive(Serialize)]
pub struct ProxyError {
    pub error: String,
    pub code: u16,
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self)).into_response()
    }
}

pub async fn chat_completions_handler<
    D: Database + Clone + Send + Sync + 'static,
    C: ConfigStore,
>(
    State(state): State<AppState<D, C>>,
    headers: axum::http::HeaderMap,
    Json(mut request): Json<hyperinfer_core::ChatRequest>,
) -> Result<Response, ProxyError> {
    let start = std::time::Instant::now();

    // 1. Extract and validate API key
    let api_key = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");

    let auth = match proxy::validate_api_key(&state.db, api_key).await {
        Ok(auth) => auth,
        Err(status) => {
            return Err(ProxyError {
                error: "Invalid or missing API key".to_string(),
                code: status,
            })
        }
    };

    // 1a. Check virtual key model allowlist and budget
    auth.check_request_allowed(&request.model)
        .map_err(|(code, error)| ProxyError { error, code })?;

    // 1b. Run GatewayPlugin pre-request pipeline
    let mut plugin_ctx = hyperinfer_core::PluginContext {
        request_id: uuid::Uuid::new_v4().to_string(),
        team_id: Some(auth.team_id.clone()),
        api_key_id: Some(auth.api_key_id.clone()),
        model: request.model.clone(),
        is_fallback: false,
        metadata: std::collections::HashMap::new(),
    };
    match state
        .plugins
        .run_pre_request(&mut plugin_ctx, &mut request)
        .await
    {
        hyperinfer_core::PluginAction::Continue => {}
        hyperinfer_core::PluginAction::ShortCircuit {
            status_code,
            message,
        } => {
            return Err(ProxyError {
                error: message,
                code: status_code,
            });
        }
        hyperinfer_core::PluginAction::Respond(mut resp) => {
            state
                .plugins
                .run_post_response(&mut plugin_ctx, &mut resp)
                .await;
            return Ok(Json(resp).into_response());
        }
    }

    // 2. Enforce team RPM / TPM quotas
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
                    Err(e) => {
                        tracing::warn!(team_id = %auth.team_id, error = %e, "RPM quota check failed");
                    }
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
                    Err(e) => {
                        tracing::warn!(team_id = %auth.team_id, error = %e, "TPM quota check failed");
                    }
                    Ok(true) => {}
                }
            }
        }
    }

    // Enforce per-key RPM / TPM limits
    if let Some(ref limiter) = state.rate_limiter {
        if let Some(key_rpm) = auth.rpm_limit {
            if key_rpm > 0 {
                match limiter
                    .check_rpm(&format!("key:{}", auth.api_key_id), key_rpm as u64)
                    .await
                {
                    Ok((false, _)) => {
                        return Err(ProxyError {
                            error: "API key RPM limit exceeded".to_string(),
                            code: 429,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(api_key_id = %auth.api_key_id, error = %e, "Key RPM check failed");
                    }
                    Ok((true, _)) => {}
                }
            }
        }
        if let Some(key_tpm) = auth.tpm_limit {
            if key_tpm > 0 {
                match limiter
                    .check_tpm(
                        &format!("key:{}", auth.api_key_id),
                        key_tpm as u64,
                        estimated_tokens,
                    )
                    .await
                {
                    Ok(false) => {
                        return Err(ProxyError {
                            error: "API key TPM limit exceeded".to_string(),
                            code: 429,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(api_key_id = %auth.api_key_id, error = %e, "Key TPM check failed");
                    }
                    Ok(true) => {}
                }
            }
        }
    }

    // 3. Load active deployments from database
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

    // 4. Select deployment using the shared ProxyRouter
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

    // Apply deployment mutators (model override, max_tokens cap, temperature override).
    let mut request = request;
    selected.mutators.apply_to_request(&mut request);

    let deployment_id = selected.deployment.id.clone();

    // 5. Streaming path (`stream: true`) via ProviderRegistry + SSE
    if request.stream == Some(true) {
        let stream = match proxy::forward_chat_stream(
            proxy_router.providers(),
            &request,
            &selected.base_url,
            &selected.api_key,
            &selected.provider,
            proxy_router.allows_localhost(),
        )
        .await
        {
            Ok(s) => s,
            Err(code) => {
                if code == 429 {
                    proxy_router.record_quota_exhausted(&deployment_id).await;
                } else {
                    proxy_router.record_failure(&deployment_id).await;
                }
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

            let mut stream_failed = false;
            while let Some(item) = stream.next().await {
                match item {
                    Ok(chunk) => {
                        if let Some(ref usage) = chunk.usage {
                            input_tokens = usage.input_tokens;
                            output_tokens = usage.output_tokens;
                        }
                        if let Ok(json) = serde_json::to_string(&chunk) {
                            yield Ok::<SseEvent, std::convert::Infallible>(SseEvent::default().data(json));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Error in upstream SSE chunk");
                        router_clone.record_failure(&deployment_id).await;
                        stream_failed = true;
                        break;
                    }
                }
            }

            if stream_failed {
                yield Ok::<SseEvent, std::convert::Infallible>(
                    SseEvent::default()
                        .event("error")
                        .data("Upstream streaming request failed"),
                );
                return;
            }

            yield Ok::<SseEvent, std::convert::Infallible>(SseEvent::default().data("[DONE]"));

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

            // Charge weighted cost to API key
            let cost_cents = hyperinfer_core::CostCalculator::with_defaults()
                .calculate_cost_cents_ceil(
                    &model_name,
                    &hyperinfer_core::WeightedUsage {
                        input_tokens: input_tokens as u64,
                        cached_input_tokens: 0,
                        output_tokens: output_tokens as u64,
                        reasoning_tokens: 0,
                    },
                );
            if cost_cents > 0 {
                if let Err(e) = db.record_api_key_spend(&api_key_id, cost_cents).await {
                    tracing::warn!(
                        api_key_id = %api_key_id,
                        cost_cents,
                        error = ?e,
                        "Failed to record streaming API key spend"
                    );
                }
            }
        };

        return Ok(Sse::new(sse_stream)
            .keep_alive(KeepAlive::default())
            .into_response());
    }

    // 6. Non-streaming path via ProviderRegistry
    let mut response = match proxy::forward_chat(
        proxy_router.providers(),
        &request,
        &selected.base_url,
        &selected.api_key,
        &selected.provider,
        proxy_router.allows_localhost(),
    )
    .await
    {
        Ok(resp) => resp,
        Err(code) => {
            if code == 429 {
                proxy_router.record_quota_exhausted(&deployment_id).await;
            } else {
                proxy_router.record_failure(&deployment_id).await;
            }
            state
                .plugins
                .run_on_error(
                    &mut plugin_ctx,
                    &format!("Upstream request failed ({code})"),
                )
                .await;
            return Err(ProxyError {
                error: "Upstream request failed".to_string(),
                code,
            });
        }
    };

    state
        .plugins
        .run_post_response(&mut plugin_ctx, &mut response)
        .await;

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
                tracing::warn!(team_id = %auth.team_id, error = %e, "Post-response TPM accounting failed");
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
        tracing::warn!(error = ?e, "Failed to record proxy usage log");
    }

    // Charge weighted cost to API key
    let cost_cents = hyperinfer_core::CostCalculator::with_defaults().calculate_cost_cents_ceil(
        &request.model,
        &hyperinfer_core::WeightedUsage {
            input_tokens: response.usage.input_tokens as u64,
            cached_input_tokens: 0,
            output_tokens: response.usage.output_tokens as u64,
            reasoning_tokens: 0,
        },
    );
    if cost_cents > 0 {
        let api_key_id = auth.api_key_id.clone();
        if let Err(e) = state.db.record_api_key_spend(&api_key_id, cost_cents).await {
            tracing::warn!(
                api_key_id = %api_key_id,
                cost_cents,
                error = ?e,
                "Failed to record API key spend"
            );
        }
    }

    Ok(Json(response).into_response())
}
