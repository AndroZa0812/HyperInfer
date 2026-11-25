//! End-to-end integration tests for the cross-repo shared memory service (HYP-44).

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::http::StatusCode;
use axum_test::TestServer;
use chrono::Utc;
use hyperinfer_core::{
    Config, ConfigError, ConfigStore, Memory, MemoryEventAction, MemorySearchResult,
    MemoryUpdateEvent, PolicyUpdate, User,
};
use hyperinfer_server::auth::create_auth_token;
use hyperinfer_server::standalone::memory_db::InMemoryDb;
use tokio::sync::RwLock;

const JWT_SECRET: &str = "test-memories-jwt-secret";

#[derive(Clone, Default)]
struct SpyConfigStore {
    events: Arc<Mutex<Vec<MemoryUpdateEvent>>>,
}

#[async_trait]
impl ConfigStore for SpyConfigStore {
    async fn fetch_config(&self) -> Result<Config, ConfigError> {
        Ok(Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        })
    }

    async fn publish_config_update(&self, _config: &Config) -> Result<(), ConfigError> {
        Ok(())
    }

    async fn publish_policy_update(&self, _update: &PolicyUpdate) -> Result<(), ConfigError> {
        Ok(())
    }

    async fn publish_memory_event(&self, event: &MemoryUpdateEvent) -> Result<(), ConfigError> {
        self.events.lock().unwrap().push(event.clone());
        Ok(())
    }

    async fn ping(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}

fn make_token(user_id: &str, team_id: &str, role: &str) -> String {
    let user = User {
        id: user_id.to_string(),
        team_id: team_id.to_string(),
        email: format!("{user_id}@example.com"),
        role: role.to_string(),
        password_hash: None,
        created_at: Utc::now(),
    };
    create_auth_token(&user, JWT_SECRET, 3600).unwrap()
}

fn build_test_server() -> (TestServer, SpyConfigStore) {
    let db = InMemoryDb::new();
    let spy = SpyConfigStore::default();
    let config = Config {
        api_keys: std::collections::HashMap::new(),
        routing_rules: Vec::new(),
        quotas: std::collections::HashMap::new(),
        model_aliases: std::collections::HashMap::new(),
        default_provider: None,
    };
    let state = hyperinfer_server::AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager: spy.clone(),
        admin_token: Arc::new("admin-token".to_string()),
        jwt_secret: Arc::new(JWT_SECRET.to_string()),
        proxy_router: None,
        rate_limiter: None,
        plugins: hyperinfer_core::PluginPipeline::new(),
    };
    let app = hyperinfer_server::build_router(
        state,
        hyperinfer_server::mcp::McpState::new(JWT_SECRET),
        tower_http::cors::CorsLayer::permissive(),
        false,
    );
    (TestServer::new(app), spy)
}

#[tokio::test]
async fn test_memories_crud_search_ranking_and_events() {
    let (server, spy) = build_test_server();
    let admin_tok = make_token("user-a", "team-a", "admin");

    // 1. Create two repo-scoped memories
    let res1 = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .json(&serde_json::json!({
            "content": "Always run cargo clippy with -D warnings before pushing to main",
            "scope": "repo",
            "repo_id": "AndroZa0812/HyperInfer",
            "tags": ["rust", "ci", "clippy"]
        }))
        .await;
    assert_eq!(res1.status_code(), StatusCode::CREATED);
    let m1: Memory = res1.json();
    assert_eq!(m1.scope, "repo");
    assert_eq!(m1.status, "active");

    let res2 = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .json(&serde_json::json!({
            "content": "Use Svelte 5 runes in apps/dashboard for reactive state",
            "scope": "repo",
            "repo_id": "AndroZa0812/HyperInfer",
            "tags": ["svelte", "dashboard"]
        }))
        .await;
    assert_eq!(res2.status_code(), StatusCode::CREATED);
    let m2: Memory = res2.json();

    // 2. Search ranking: query for rust clippy should rank m1 first
    let search_res = server
        .post("/v1/memories/search")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .json(&serde_json::json!({
            "query": "rust cargo clippy",
            "repo_id": "AndroZa0812/HyperInfer",
            "limit": 10
        }))
        .await;
    assert_eq!(search_res.status_code(), StatusCode::OK);
    let hits: Vec<MemorySearchResult> = search_res.json();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, m1.id);
    assert!(hits[0].score > 0.0);

    // 3. List with tag filter
    let list_res = server
        .get("/v1/memories?tag=svelte")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .await;
    assert_eq!(list_res.status_code(), StatusCode::OK);
    let listed: Vec<Memory> = list_res.json();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, m2.id);

    // 4. Soft-expire via DELETE -> excluded from default list and search
    let del_res = server
        .delete(&format!("/v1/memories/{}", m1.id))
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .await;
    assert_eq!(del_res.status_code(), StatusCode::OK);
    let expired: Memory = del_res.json();
    assert_eq!(expired.status, "expired");
    assert!(expired.valid_to.is_some());

    let search_after = server
        .post("/v1/memories/search")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .json(&serde_json::json!({ "query": "clippy" }))
        .await;
    let hits_after: Vec<MemorySearchResult> = search_after.json();
    assert!(hits_after.is_empty());

    // 5. Verify Redis invalidation events were emitted
    let events = spy.events.lock().unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].action, MemoryEventAction::Created);
    assert_eq!(events[1].action, MemoryEventAction::Created);
    assert_eq!(events[2].action, MemoryEventAction::Expired);
}

#[tokio::test]
async fn test_memories_temporal_superseding() {
    let (server, spy) = build_test_server();
    let admin_tok = make_token("user-a", "team-a", "admin");

    let old_res = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .json(&serde_json::json!({
            "content": "Default database port is 5432",
            "scope": "team",
            "tags": ["postgres"]
        }))
        .await;
    assert_eq!(old_res.status_code(), StatusCode::CREATED);
    let old_mem: Memory = old_res.json();

    let new_res = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .json(&serde_json::json!({
            "content": "Default database port is 5433 in staging",
            "scope": "team",
            "tags": ["postgres", "staging"],
            "supersedes_id": old_mem.id
        }))
        .await;
    assert_eq!(new_res.status_code(), StatusCode::CREATED);
    let new_mem: Memory = new_res.json();

    // Old memory should now be expired with superseded_by pointing to new_mem.id
    let get_old = server
        .get(&format!("/v1/memories/{}", old_mem.id))
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .await;
    assert_eq!(get_old.status_code(), StatusCode::OK);
    let old_updated: Memory = get_old.json();
    assert_eq!(old_updated.status, "expired");
    assert!(old_updated.valid_to.is_some());
    assert_eq!(
        old_updated.superseded_by.as_deref(),
        Some(new_mem.id.as_str())
    );

    // Querying ?status=expired returns the expired memory without requiring include_expired=true
    let expired_list = server
        .get("/v1/memories?status=expired")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .await;
    assert_eq!(expired_list.status_code(), StatusCode::OK);
    let expired_items: Vec<Memory> = expired_list.json();
    assert_eq!(expired_items.len(), 1);
    assert_eq!(expired_items[0].id, old_mem.id);

    // Reactivating a superseded memory is rejected with 409 Conflict
    let reactivate = server
        .patch(&format!("/v1/memories/{}", old_mem.id))
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .json(&serde_json::json!({ "status": "active" }))
        .await;
    assert_eq!(reactivate.status_code(), StatusCode::CONFLICT);

    // Search should only return the active superseding memory
    let search = server
        .post("/v1/memories/search")
        .add_header("Authorization", format!("Bearer {admin_tok}"))
        .json(&serde_json::json!({ "query": "database port" }))
        .await;
    let hits: Vec<MemorySearchResult> = search.json();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].memory.id, new_mem.id);

    // Verify Superseded event was emitted
    let events = spy.events.lock().unwrap();
    assert_eq!(events.last().unwrap().action, MemoryEventAction::Superseded);
}

#[tokio::test]
async fn test_memories_tenant_isolation_and_rbac_approval() {
    let (server, spy) = build_test_server();
    let team_a_admin = make_token("admin-a", "team-a", "admin");
    let team_a_member1 = make_token("member-a1", "team-a", "member");
    let team_a_member2 = make_token("member-a2", "team-a", "member");
    let team_b_admin = make_token("admin-b", "team-b", "admin");

    // 1. Member A1 creates a private user-scoped memory
    let priv_res = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {team_a_member1}"))
        .json(&serde_json::json!({
            "content": "My personal editor uses vim keybindings",
            "scope": "user"
        }))
        .await;
    assert_eq!(priv_res.status_code(), StatusCode::CREATED);
    let priv_mem: Memory = priv_res.json();

    // Member A2 cannot read or search Member A1's private user-scoped memory
    let get_by_a2 = server
        .get(&format!("/v1/memories/{}", priv_mem.id))
        .add_header("Authorization", format!("Bearer {team_a_member2}"))
        .await;
    assert_eq!(get_by_a2.status_code(), StatusCode::FORBIDDEN);

    let search_by_a2 = server
        .post("/v1/memories/search")
        .add_header("Authorization", format!("Bearer {team_a_member2}"))
        .json(&serde_json::json!({ "query": "vim keybindings" }))
        .await;
    let hits_a2: Vec<MemorySearchResult> = search_by_a2.json();
    assert!(hits_a2.is_empty());

    // Team B admin cannot read, edit, or delete Team A's memory (no cross-team IDOR)
    let get_by_b = server
        .get(&format!("/v1/memories/{}", priv_mem.id))
        .add_header("Authorization", format!("Bearer {team_b_admin}"))
        .await;
    assert_eq!(get_by_b.status_code(), StatusCode::FORBIDDEN);

    let del_by_b = server
        .delete(&format!("/v1/memories/{}", priv_mem.id))
        .add_header("Authorization", format!("Bearer {team_b_admin}"))
        .await;
    assert_eq!(del_by_b.status_code(), StatusCode::FORBIDDEN);

    // 2. Member A1 submits a pending_approval team memory
    let pending_res = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {team_a_member1}"))
        .json(&serde_json::json!({
            "content": "All production migrations must be backward compatible",
            "scope": "team",
            "status": "pending_approval"
        }))
        .await;
    assert_eq!(pending_res.status_code(), StatusCode::CREATED);
    let pending_mem: Memory = pending_res.json();
    assert_eq!(pending_mem.status, "pending_approval");

    // Non-admin cannot self-approve
    let self_approve = server
        .patch(&format!("/v1/memories/{}", pending_mem.id))
        .add_header("Authorization", format!("Bearer {team_a_member1}"))
        .json(&serde_json::json!({ "status": "active" }))
        .await;
    assert_eq!(self_approve.status_code(), StatusCode::FORBIDDEN);

    // Team A admin approves -> status becomes active and emits Approved event
    let admin_approve = server
        .patch(&format!("/v1/memories/{}", pending_mem.id))
        .add_header("Authorization", format!("Bearer {team_a_admin}"))
        .json(&serde_json::json!({ "status": "active" }))
        .await;
    assert_eq!(admin_approve.status_code(), StatusCode::OK);
    let approved: Memory = admin_approve.json();
    assert_eq!(approved.status, "active");

    let events = spy.events.lock().unwrap();
    assert_eq!(events.last().unwrap().action, MemoryEventAction::Approved);
}

#[tokio::test]
async fn test_memories_validation_errors() {
    let (server, _spy) = build_test_server();
    let tok = make_token("user-a", "team-a", "admin");

    // Empty content -> 400
    let res = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {tok}"))
        .json(&serde_json::json!({ "content": "   ", "scope": "team" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);

    // Invalid scope -> 400
    let res = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {tok}"))
        .json(&serde_json::json!({ "content": "hello", "scope": "global" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);

    // scope == "repo" without repo_id -> 400
    let res = server
        .post("/v1/memories")
        .add_header("Authorization", format!("Bearer {tok}"))
        .json(&serde_json::json!({ "content": "hello", "scope": "repo" }))
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);

    // Empty search query -> 400
    let res = server
        .post("/v1/memories/search")
        .add_header("Authorization", format!("Bearer {tok}"))
        .json(&serde_json::json!({ "query": "   " }))
        .await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}
