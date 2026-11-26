//! Request Logs Explorer API routes (HYP-71).
//!
//! Exposes `GET /v1/usage/logs` with pagination, multi-field filtering,
//! strict tenant scoping for non-admin users, and dynamic cost estimation
//! when persisted `cost_cents` is zero. Never returns raw prompt/response
//! payloads so redacted PII/secrets cannot leak in the dashboard UI.

use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use chrono::{DateTime, Utc};
use hyperinfer_core::{
    ConfigStore, CostCalculator, Database, DbError, ListUsageLogsFilter, WeightedUsage,
};
use serde::Deserialize;

use crate::auth::AuthClaims;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct UsageLogsQuery {
    pub team_id: Option<String>,
    pub api_key_id: Option<String>,
    pub model: Option<String>,
    pub status_code: Option<i16>,
    pub cache_hit: Option<bool>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `GET /v1/usage/logs` — Paginated request logs with filtering and tenant isolation.
pub async fn list_usage_logs<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Extension(claims): Extension<AuthClaims>,
    Query(query): Query<UsageLogsQuery>,
) -> impl IntoResponse {
    // Non-admins are always scoped to their own team (prevents cross-tenant IDOR).
    let team_id = if claims.role == "admin" {
        query.team_id.filter(|s| !s.trim().is_empty())
    } else {
        Some(claims.team_id.clone())
    };

    let filter = ListUsageLogsFilter {
        team_id,
        api_key_id: query.api_key_id.filter(|s| !s.trim().is_empty()),
        model: query.model.filter(|s| !s.trim().is_empty()),
        status_code: query.status_code,
        cache_hit: query.cache_hit,
        from: query.from,
        to: query.to,
        limit: query.limit.unwrap_or(50).clamp(1, 200),
        offset: query.offset.unwrap_or(0).max(0),
    };

    match state.db.list_usage_logs(filter).await {
        Ok(mut page) => {
            let calc = CostCalculator::with_defaults();
            for log in &mut page.logs {
                if log.cost_cents <= 0.0 && (log.input_tokens > 0 || log.output_tokens > 0) {
                    let usage = WeightedUsage {
                        input_tokens: log.input_tokens.max(0) as u64,
                        cached_input_tokens: log.cached_input_tokens.max(0) as u64,
                        output_tokens: log.output_tokens.max(0) as u64,
                        reasoning_tokens: log.reasoning_tokens.max(0) as u64,
                    };
                    log.cost_cents = calc.calculate_cost_cents(&log.model, &usage);
                }
            }
            Json(page).into_response()
        }
        Err(DbError::InvalidUuid) => (StatusCode::BAD_REQUEST, "Invalid UUID").into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to list usage logs",
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{auth_middleware, create_auth_token};
    use crate::standalone::{InMemoryConfigStore, InMemoryDb};
    use axum::{middleware, routing::get, Router};
    use axum_test::TestServer;
    use hyperinfer_core::{Config, PluginPipeline};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    async fn setup_test_server() -> (TestServer, String, String, String, String) {
        let db = InMemoryDb::new();
        let t1 = db.create_team("Team A", 10_000).await.unwrap();
        let t2 = db.create_team("Team B", 10_000).await.unwrap();
        let admin = db
            .create_user(&t1.id, "admin@test.local", "admin", None)
            .await
            .unwrap();
        let member = db
            .create_user(&t2.id, "member@test.local", "member", None)
            .await
            .unwrap();
        let k1 = db
            .create_api_key("hash1", &admin.id, &t1.id, Some("k1".into()))
            .await
            .unwrap();
        let k2 = db
            .create_api_key("hash2", &member.id, &t2.id, Some("k2".into()))
            .await
            .unwrap();

        db.record_usage(&t1.id, &k1.id, "gpt-4o", 1000, 500, 120)
            .await
            .unwrap();
        db.record_usage(&t1.id, &k1.id, "claude-sonnet-4", 2000, 1000, 240)
            .await
            .unwrap();
        db.record_usage(&t2.id, &k2.id, "gemini-2.5-flash", 500, 250, 80)
            .await
            .unwrap();

        let secret = "logs-test-secret";
        let config = Arc::new(RwLock::new(Config {
            api_keys: HashMap::new(),
            routing_rules: Vec::new(),
            quotas: HashMap::new(),
            model_aliases: HashMap::new(),
            default_provider: None,
        }));
        let state = AppState {
            config: config.clone(),
            db,
            config_manager: InMemoryConfigStore::new(config),
            admin_token: Arc::new("admin-tok".to_string()),
            jwt_secret: Arc::new(secret.to_string()),
            proxy_router: None,
            rate_limiter: None,
            plugins: PluginPipeline::new(),
        };

        let app = Router::new()
            .route(
                "/v1/usage/logs",
                get(list_usage_logs::<InMemoryDb, InMemoryConfigStore>),
            )
            .layer(middleware::from_fn_with_state(
                Arc::new(secret.to_string()),
                auth_middleware,
            ))
            .with_state(state);

        let admin_tok = create_auth_token(&admin, secret, 3600).unwrap();
        let member_tok = create_auth_token(&member, secret, 3600).unwrap();
        (TestServer::new(app), admin_tok, member_tok, t1.id, t2.id)
    }

    #[tokio::test]
    async fn test_usage_logs_filtering_pagination_and_tenant_isolation() {
        let (server, admin_tok, member_tok, t1_id, t2_id) = setup_test_server().await;

        // Admin sees all 3 logs with calculated cost_cents > 0
        let all_resp = server
            .get("/v1/usage/logs?limit=2&offset=0")
            .add_header("Authorization", format!("Bearer {admin_tok}"))
            .await;
        all_resp.assert_status_ok();
        let page: hyperinfer_core::UsageLogsPage = all_resp.json();
        assert_eq!(page.total, 3);
        assert_eq!(page.logs.len(), 2);
        assert!(page.logs.iter().all(|l| l.cost_cents > 0.0));

        // Admin filters by model substring
        let model_resp = server
            .get("/v1/usage/logs?model=claude")
            .add_header("Authorization", format!("Bearer {admin_tok}"))
            .await;
        model_resp.assert_status_ok();
        let model_page: hyperinfer_core::UsageLogsPage = model_resp.json();
        assert_eq!(model_page.total, 1);
        assert_eq!(model_page.logs[0].model, "claude-sonnet-4");

        // Member attempting to query Team A's logs still only receives their own team (Team B)
        let member_resp = server
            .get(&format!("/v1/usage/logs?team_id={t1_id}"))
            .add_header("Authorization", format!("Bearer {member_tok}"))
            .await;
        member_resp.assert_status_ok();
        let member_page: hyperinfer_core::UsageLogsPage = member_resp.json();
        assert_eq!(member_page.total, 1);
        assert_eq!(member_page.logs[0].team_id, t2_id);
        assert_eq!(member_page.logs[0].model, "gemini-2.5-flash");
    }
}
