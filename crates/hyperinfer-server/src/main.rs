//! HyperInfer Server (Control Plane)

use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use hyperinfer_core::{Config, ConfigStore, Database};
use hyperinfer_server::{RedisConfigStore, SqlxDb};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tracing::info;

#[derive(Clone)]
struct AppState<D: Database, C: ConfigStore> {
    config: Arc<RwLock<Config>>,
    db: D,
    #[allow(dead_code)]
    config_manager: C,
}

type ProdState = AppState<SqlxDb, RedisConfigStore>;

/// Returns the in-memory configuration currently held in the application state.
///
/// Reads and clones the shared `Config` from the state's `config` RwLock and returns it as JSON.
///
/// # Examples
///
/// ```
/// // In an async handler or test with access to `state: State<AppState<_, _>>`:
/// // let response = config_sync(state).await;
/// // The response contains the current `Config` serialized as JSON.
/// ```
async fn config_sync<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
) -> impl IntoResponse {
    let config = state.config.read().await;
    Json(config.clone())
}

/// Fetches a team by its ID from the application's database and returns an HTTP response.
///
/// On success returns the team as JSON. If the team is not found returns 404 with the
/// message "Team not found". If the database operation fails returns 500 with the
/// message "Database error".
///
/// # Examples
///
/// ```
/// # use axum::extract::{State, Path};
/// # use axum::response::IntoResponse;
/// # use axum::http::StatusCode;
/// # // `create_test_state` and `MockDatabase`/`MockConfigStore` are provided by the test helpers in this crate.
/// # use crate::tests::create_test_state;
/// #[tokio::test]
/// async fn example_get_team_not_found() {
///     let state = create_test_state();
///     let resp = super::get_team(State(state), Path("nonexistent".to_string())).await.into_response();
///     assert_eq!(resp.status(), StatusCode::NOT_FOUND);
/// }
/// ```
async fn get_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_team(&team_id).await {
        Ok(Some(team)) => Json(team).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Team not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

/// Creates a new team with the provided name and budget.
///
/// On success returns the created `Team` as JSON; on failure returns a 500 response with a
/// "Failed to create team" message.
///
/// # Examples
///
/// ```
/// # use axum::{extract::State, extract::Json};
/// # use hyperinfer_server::{create_team, AppState, CreateTeamRequest};
/// # // The following is a conceptual example; in tests you would construct an AppState with a mock Database.
/// # async fn example(state: State<AppState<impl hyperinfer_core::Database, impl hyperinfer_core::ConfigStore>>) {
/// let req = Json(CreateTeamRequest { name: "acme".into(), budget_cents: 1_000_00 });
/// let response = create_team(state, req).await;
/// // `response` is an HTTP response: 200 with JSON body on success, 500 with error message on failure.
/// # }
/// ```
async fn create_team<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateTeamRequest>,
) -> impl IntoResponse {
    match state.db.create_team(&req.name, req.budget_cents).await {
        Ok(team) => Json(team).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create team").into_response(),
    }
}

/// Fetches a user by ID and returns an appropriate HTTP response.
///
/// Returns a JSON-encoded user with status 200 when the user exists, a 404 status with the message
/// "User not found" when no user is found, or a 500 status with the message "Database error" on
/// database failures.
///
/// # Examples
///
/// ```
/// # use axum::extract::{State, Path};
/// # async fn example() {
/// // Construct a test AppState with a mock Database and ConfigStore, then:
/// // let state = AppState { config: ..., db: mock_db, config_manager: mock_cfg };
/// // let response = get_user(State(state), Path("user-123".to_string())).await;
/// # }
/// ```
async fn get_user<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_user(&user_id).await {
        Ok(Some(user)) => Json(user).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "User not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

/// Creates a new user for the given team and returns the created user on success.
///
/// On success the response contains the created user serialized as JSON. On failure the response
/// is a 500 Internal Server Error with the message "Failed to create user".
///
/// # Examples
///
/// ```
/// // Assume `state` is an `AppState` with a test `Database` and `ConfigStore`.
/// // let state = create_test_state();
/// let req = CreateUserRequest {
///     team_id: "team1".into(),
///     email: "alice@example.com".into(),
///     role: "member".into(),
/// };
///
/// // Call the handler (in an async context)
/// // let resp = create_user(State(state), Json(req)).await;
/// // assert!(resp.status().is_success());
/// ```
async fn create_user<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateUserRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_user(&req.team_id, &req.email, &req.role)
        .await
    {
        Ok(user) => Json(user).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create user").into_response(),
    }
}

/// Fetches an API key by its ID and returns an HTTP response.
///
/// Calls the configured database to retrieve the API key and maps outcomes to HTTP responses.
///
/// # Returns
///
/// An HTTP response containing the API key as JSON on success; `404 Not Found` with the message
/// "API key not found" if no key exists for the given ID; `500 Internal Server Error` with the
/// message "Database error" if the database query fails.
///
/// # Examples
///
/// ```no_run
/// use axum::response::IntoResponse;
/// use axum::extract::State;
/// use axum::extract::Path;
///
/// // `state` must be an AppState implementing the required traits; this example is illustrative.
/// # async fn example<D, C>(state: State<crate::AppState<D, C>>) where D: crate::Database, C: crate::ConfigStore {
/// let resp = crate::get_api_key::<D, C>(state, Path("api_key_id".to_string())).await.into_response();
/// // match on the response status or body as needed
/// # }
/// ```
async fn get_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(key_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_api_key(&key_id).await {
        Ok(Some(key)) => Json(key).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "API key not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

/// Creates a new API key for the specified user and team.
///
/// On success, returns an HTTP response containing the created API key as JSON.
/// On failure, returns a 500 Internal Server Error with the message "Failed to create API key".
///
/// # Examples
///
/// ```no_run
/// use axum::Json;
/// use hyperinfer_server::CreateApiKeyRequest;
///
/// // Build a request body and let the server route invoke this handler.
/// let req = CreateApiKeyRequest {
///     key_hash: "hash".into(),
///     user_id: "user".into(),
///     team_id: "team".into(),
///     name: Some("my-key".into()),
/// };
///
/// // Handler invocation is performed by the Axum router in normal usage:
/// // POST /v1/api_keys with JSON body -> create_api_key
/// let _ = Json(req);
/// ```
async fn create_api_key<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateApiKeyRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_api_key(&req.key_hash, &req.user_id, &req.team_id, req.name)
        .await
    {
        Ok(key) => Json(key).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create API key",
        )
            .into_response(),
    }
}

/// Fetches a model alias by its identifier and maps the result to an HTTP response.
///
/// Returns `200` with the alias as JSON if found, `404` with the text "Model alias not found" if no alias exists for the given id, or `500` with the text "Database error" if the database query fails.
///
/// # Examples
///
/// ```no_run
/// use axum::{extract::State, extract::Path};
/// // `state` and `alias_id` would be provided by the Axum runtime in real usage.
/// // get_model_alias(State(state), Path(alias_id)).await;
/// ```
async fn get_model_alias<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(alias_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_model_alias(&alias_id).await {
        Ok(Some(alias)) => Json(alias).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Model alias not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

/// Creates a model alias for a team and returns an HTTP response.
///
/// On success returns a 200 OK response with the created model alias serialized as JSON.
/// If the database operation fails returns a 500 Internal Server Error with the message
/// "Failed to create model alias".
///
/// # Examples
///
/// ```no_run
/// use axum::response::IntoResponse;
/// use hyperinfer_server::CreateModelAliasRequest;
///
/// // When mounted in the router, calling the endpoint with a valid request
/// // results in a 200 response containing the created model alias as JSON,
/// // or a 500 response with the text "Failed to create model alias" on error.
/// let _req = CreateModelAliasRequest {
///     team_id: "team-123".into(),
///     alias: "my-alias".into(),
///     target_model: "gpt-4".into(),
///     provider: "openai".into(),
/// };
/// ```
async fn create_model_alias<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateModelAliasRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_model_alias(&req.team_id, &req.alias, &req.target_model, &req.provider)
        .await
    {
        Ok(alias) => Json(alias).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Failed to create model alias",
        )
            .into_response(),
    }
}

/// Fetches the quota for the given team and returns an HTTP response.
///
/// On success returns the quota serialized as JSON; if no quota exists for the
/// team returns HTTP 404 with "Quota not found"; on database errors returns
/// HTTP 500 with "Database error".
///
/// # Examples
///
/// ```no_run
/// use axum::extract::{State, Path};
/// # async fn example() {
/// // `state` must be an `AppState` with a `Database` implementation.
/// let state = /* AppState::<_, _> */ unimplemented!();
/// let resp = get_quota(State(state), Path("team-id".to_string())).await;
/// # }
/// ```
async fn get_quota<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Path(team_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_quota(&team_id).await {
        Ok(Some(quota)) => Json(quota).into_response(),
        Ok(None) => (StatusCode::NOT_FOUND, "Quota not found").into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    }
}

/// Creates a quota for a team.
///
/// Attempts to create a quota record using the provided request and returns the created quota as JSON on success.
/// On failure, responds with HTTP 500 and a plain text error message.
///
/// # Returns
///
/// `Json<Quota>` containing the created quota on success, or a `(StatusCode::INTERNAL_SERVER_ERROR, &str)` response on failure.
///
/// # Examples
///
/// ```
/// # // The following is a hidden example outline; actual construction of `state` depends on the test helpers in this crate.
/// # use axum::Json;
/// # use hyperinfer_core::CreateQuotaRequest;
/// # async fn example_call(state: crate::AppState<impl crate::Database, impl crate::ConfigStore>) {
/// let req = CreateQuotaRequest {
///     team_id: "team-123".to_string(),
///     rpm_limit: 100,
///     tpm_limit: 1000,
/// };
/// // call the handler (async) and inspect the response
/// let _resp = crate::create_quota(axum::extract::State(state), Json(req)).await;
/// # }
/// ```
async fn create_quota<D: Database, C: ConfigStore>(
    State(state): State<AppState<D, C>>,
    Json(req): Json<CreateQuotaRequest>,
) -> impl IntoResponse {
    match state
        .db
        .create_quota(&req.team_id, req.rpm_limit, req.tpm_limit)
        .await
    {
        Ok(quota) => Json(quota).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create quota").into_response(),
    }
}

#[derive(Deserialize)]
struct CreateTeamRequest {
    name: String,
    budget_cents: i64,
}

#[derive(Deserialize)]
struct CreateUserRequest {
    team_id: String,
    email: String,
    role: String,
}

#[derive(Deserialize)]
struct CreateApiKeyRequest {
    key_hash: String,
    user_id: String,
    team_id: String,
    name: Option<String>,
}

#[derive(Deserialize)]
struct CreateModelAliasRequest {
    team_id: String,
    alias: String,
    target_model: String,
    provider: String,
}

#[derive(Deserialize)]
struct CreateQuotaRequest {
    team_id: String,
    rpm_limit: i32,
    tpm_limit: i32,
}

/// Initializes logging, database, config store, HTTP routes, and starts the control-plane server.
///
/// This function:
/// - configures tracing subscriber for logging,
/// - reads DATABASE_URL and REDIS_URL (with sensible defaults),
/// - creates a Postgres connection pool and runs SQL migrations,
/// - initializes a SqlxDb and a RedisConfigStore, loading stored config or falling back to an empty default,
/// - constructs application state and CORS policy (driven by ALLOWED_ORIGINS, defaulting to http://localhost:3000),
/// - registers REST routes for config, teams, users, API keys, model aliases, and quotas,
/// - binds to 0.0.0.0:3000 and serves the Axum application.
///
/// # Examples
///
/// ```no_run
/// // Run the server (sets defaults for DATABASE_URL and REDIS_URL when not provided).
/// // From a shell:
/// //   DATABASE_URL="postgres://..." REDIS_URL="redis://..." cargo run --bin hyperinfer-server
/// ```
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::init();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/hyperinfer".to_string());

    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;

    let db = SqlxDb::new(pool);
    let config_manager = RedisConfigStore::new(&redis_url).await?;
    let config = config_manager.fetch_config().await.unwrap_or_else(|e| {
        tracing::warn!(
            "Failed to fetch config from Redis, starting with empty config: {:?}",
            e
        );
        Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        }
    });

    let state: ProdState = AppState {
        config: Arc::new(RwLock::new(config)),
        db,
        config_manager,
    };

    let cors = {
        let allowed_origins = std::env::var("ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());
        let origins: Vec<_> = allowed_origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        if origins.is_empty() {
            tracing::warn!("No valid CORS origins configured, defaulting to localhost:3000");
            CorsLayer::new().allow_origin(
                "http://localhost:3000"
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
        } else {
            CorsLayer::new().allow_origin(origins)
        }
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE])
    };

    let app = Router::new()
        .route("/v1/config/sync", get(config_sync))
        .route("/v1/teams/:id", get(get_team))
        .route("/v1/teams", post(create_team))
        .route("/v1/users/:id", get(get_user))
        .route("/v1/users", post(create_user))
        .route("/v1/api_keys/:id", get(get_api_key))
        .route("/v1/api_keys", post(create_api_key))
        .route("/v1/model_aliases/:id", get(get_model_alias))
        .route("/v1/model_aliases", post(create_model_alias))
        .route("/v1/quotas/:team_id", get(get_quota))
        .route("/v1/quotas", post(create_quota))
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Server listening on {}", listener.local_addr()?);

    axum::serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperinfer_core::{
        ApiKey, ConfigError, DbError, ModelAlias, PolicyUpdate, Quota, Team, User,
    };
    use mockall::mock;
    use mockall::predicate::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    mock! {
        pub Database {}

        impl Clone for Database {
            fn clone(&self) -> Self;
        }

        #[async_trait::async_trait]
        impl hyperinfer_core::Database for Database {
            async fn get_team(&self, id: &str) -> Result<Option<Team>, DbError>;
            async fn create_team(&self, name: &str, budget_cents: i64) -> Result<Team, DbError>;
            async fn get_user(&self, id: &str) -> Result<Option<User>, DbError>;
            async fn create_user(&self, team_id: &str, email: &str, role: &str) -> Result<User, DbError>;
            async fn get_api_key(&self, id: &str) -> Result<Option<ApiKey>, DbError>;
            async fn create_api_key(&self, key_hash: &str, user_id: &str, team_id: &str, name: Option<String>) -> Result<ApiKey, DbError>;
            async fn get_model_alias(&self, id: &str) -> Result<Option<ModelAlias>, DbError>;
            async fn create_model_alias(&self, team_id: &str, alias: &str, target_model: &str, provider: &str) -> Result<ModelAlias, DbError>;
            async fn get_quota(&self, team_id: &str) -> Result<Option<Quota>, DbError>;
            async fn create_quota(&self, team_id: &str, rpm_limit: i32, tpm_limit: i32) -> Result<Quota, DbError>;
        }
    }

    mock! {
        pub ConfigStore {}

        impl Clone for ConfigStore {
            fn clone(&self) -> Self;
        }

        #[async_trait::async_trait]
        impl hyperinfer_core::ConfigStore for ConfigStore {
            async fn fetch_config(&self) -> Result<Config, ConfigError>;
            async fn publish_config_update(&self, config: &Config) -> Result<(), ConfigError>;
            async fn publish_policy_update(&self, update: &PolicyUpdate) -> Result<(), ConfigError>;
        }
    }

    /// Constructs a test AppState preconfigured with mock implementations and an empty Config.
    ///
    /// The returned state contains:
    /// - an empty `Config` (no API keys, routing rules, quotas, or model aliases, and `default_provider` set to `None`),
    /// - a `MockDatabase` as `db`,
    /// - a `MockConfigStore` as `config_manager`.
    ///
    /// # Examples
    ///
    /// ```
    /// let _state = create_test_state();
    /// ```
    ///
    /// # Returns
    ///
    /// An `AppState<MockDatabase, MockConfigStore>` populated for use in unit tests.
    fn create_test_state() -> AppState<MockDatabase, MockConfigStore> {
        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        AppState {
            config: Arc::new(RwLock::new(config)),
            db: MockDatabase::new(),
            config_manager: MockConfigStore::new(),
        }
    }

    #[tokio::test]
    async fn test_config_sync() {
        let state = create_test_state();
        let response = config_sync(State(state)).await;
        let json = response.into_response();
        assert_eq!(json.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_team_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_team()
            .with(eq("nonexistent-id"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_team(State(state), Path("nonexistent-id".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_team_found() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();
        let team = Team {
            id: "test-team-id".to_string(),
            name: "Test Team".to_string(),
            budget_cents: 10000,
            created_at: now,
            updated_at: now,
        };
        let team_clone = team.clone();
        db.expect_get_team()
            .with(eq("test-team-id"))
            .times(1)
            .returning(move |_| Ok(Some(team_clone.clone())));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_team(State(state), Path("test-team-id".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_create_team() {
        use chrono::Utc;

        let mut db = MockDatabase::new();
        let now = Utc::now();
        let team = Team {
            id: "new-team-id".to_string(),
            name: "New Team".to_string(),
            budget_cents: 5000,
            created_at: now,
            updated_at: now,
        };
        db.expect_create_team()
            .with(eq("New Team"), eq(5000i64))
            .times(1)
            .returning(move |_, _| Ok(team.clone()));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = create_team(
            State(state),
            Json(CreateTeamRequest {
                name: "New Team".to_string(),
                budget_cents: 5000,
            }),
        )
        .await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_get_user_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_user()
            .with(eq("nonexistent-user"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_user(State(state), Path("nonexistent-user".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_api_key_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_api_key()
            .with(eq("nonexistent-key"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_api_key(State(state), Path("nonexistent-key".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_model_alias_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_model_alias()
            .with(eq("nonexistent-alias"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_model_alias(State(state), Path("nonexistent-alias".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    /// Verifies that requesting a quota for a non-existent team yields a 404 response.
    ///
    /// Sets up a mock database that returns `Ok(None)` for the requested team ID and
    /// asserts that `get_quota` responds with HTTP 404 Not Found.
    ///
    /// # Examples
    ///
    /// ```
    /// // Arrange: mock database returns no quota for "nonexistent-team"
    /// let mut db = MockDatabase::new();
    /// db.expect_get_quota()
    ///     .with(eq("nonexistent-team"))
    ///     .times(1)
    ///     .returning(|_| Ok(None));
    ///
    /// let config = Config {
    ///     api_keys: std::collections::HashMap::new(),
    ///     routing_rules: Vec::new(),
    ///     quotas: std::collections::HashMap::new(),
    ///     model_aliases: std::collections::HashMap::new(),
    ///     default_provider: None,
    /// };
    /// let state: AppState<MockDatabase, MockConfigStore> = AppState {
    ///     config: Arc::new(RwLock::new(config)),
    ///     db,
    ///     config_manager: MockConfigStore::new(),
    /// };
    ///
    /// // Act
    /// let response = get_quota(State(state), Path("nonexistent-team".to_string())).await;
    /// let resp = response.into_response();
    ///
    /// // Assert
    /// assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    /// ```
    #[tokio::test]
    async fn test_get_quota_not_found() {
        let mut db = MockDatabase::new();
        db.expect_get_quota()
            .with(eq("nonexistent-team"))
            .times(1)
            .returning(|_| Ok(None));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_quota(State(state), Path("nonexistent-team".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_team_database_error() {
        let mut db = MockDatabase::new();
        db.expect_get_team()
            .with(eq("error-id"))
            .times(1)
            .returning(|_| Err(DbError::Sqlx(sqlx::Error::Protocol("test error".into()))));

        let config = Config {
            api_keys: std::collections::HashMap::new(),
            routing_rules: Vec::new(),
            quotas: std::collections::HashMap::new(),
            model_aliases: std::collections::HashMap::new(),
            default_provider: None,
        };
        let state: AppState<MockDatabase, MockConfigStore> = AppState {
            config: Arc::new(RwLock::new(config)),
            db,
            config_manager: MockConfigStore::new(),
        };

        let response = get_team(State(state), Path("error-id".to_string())).await;
        let resp = response.into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}