use hyperinfer_core::Database;
use hyperinfer_server::SqlxDb;
use sqlx::postgres::PgPoolOptions;
use testcontainers::{runners::AsyncRunner, ContainerAsync};
use testcontainers_modules::postgres::Postgres;

/// Starts a PostgreSQL test container, applies the initial schema, and returns a connected test database wrapper and the container handle.
///
/// The returned database is ready for use (pgcrypto enabled and initial migrations applied). The container handle must be kept alive for the lifetime of the test to keep the database running.
///
/// # Examples
///
/// ```
/// # async fn run() {
/// let (db, _container) = setup_test_db().await;
/// // use `db` for test operations; `_container` keeps the Postgres instance running
/// # }
/// ```
async fn setup_test_db() -> (impl Database, ContainerAsync<Postgres>) {
    let postgres = Postgres::default()
        .start()
        .await
        .expect("Failed to start PostgreSQL container");
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let connection_string = format!("postgres://postgres:postgres@127.0.0.1:{}/postgres", port);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&connection_string)
        .await
        .expect("Failed to connect to PostgreSQL");

    sqlx::query("CREATE EXTENSION IF NOT EXISTS pgcrypto")
        .execute(&pool)
        .await
        .expect("Failed to enable pgcrypto extension");

    sqlx::raw_sql(include_str!("../migrations/001_initial_schema.sql"))
        .execute(&pool)
        .await
        .expect("Failed to run migrations");

    (SqlxDb::new(pool), postgres)
}

/// Integration test that creates a team in a temporary test database and verifies it can be retrieved with the same fields.
///
/// # Examples
///
/// ```
/// // Spins up a PostgreSQL test container, creates a team, and verifies retrieval.
/// let (db, _container) = setup_test_db().await;
///
/// let team = db
///     .create_team("Test Team", 10000)
///     .await
///     .expect("Failed to create team");
/// assert_eq!(team.name, "Test Team");
/// assert_eq!(team.budget_cents, 10000);
///
/// let fetched = db
///     .get_team(&team.id)
///     .await
///     .expect("Failed to get team")
///     .expect("Team not found");
/// assert_eq!(fetched.id, team.id);
/// assert_eq!(fetched.name, "Test Team");
/// ```
#[tokio::test]
async fn test_database_create_and_get_team() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");
    assert_eq!(team.name, "Test Team");
    assert_eq!(team.budget_cents, 10000);

    let fetched = db
        .get_team(&team.id)
        .await
        .expect("Failed to get team")
        .expect("Team not found");
    assert_eq!(fetched.id, team.id);
    assert_eq!(fetched.name, "Test Team");
}

#[tokio::test]
async fn test_database_create_and_get_user() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    let user = db
        .create_user(&team.id, "test@example.com", "admin")
        .await
        .expect("Failed to create user");
    assert_eq!(user.email, "test@example.com");
    assert_eq!(user.role, "admin");

    let fetched = db
        .get_user(&user.id)
        .await
        .expect("Failed to get user")
        .expect("User not found");
    assert_eq!(fetched.id, user.id);
    assert_eq!(fetched.email, "test@example.com");
}

/// Verifies that an API key can be created and subsequently retrieved from the test database.
///
/// This integration test creates a team and a user, inserts an API key (with a hash and optional name),
/// asserts the created key's fields (hash, name, active status), and then fetches the key by ID to
/// confirm persistence and field equality.
///
/// # Examples
///
/// ```
/// # async fn run_test_example() {
/// let (db, _container) = setup_test_db().await;
///
/// let team = db.create_team("Test Team", 10000).await.unwrap();
/// let user = db.create_user(&team.id, "test@example.com", "admin").await.unwrap();
///
/// let api_key = db
///     .create_api_key(
///         "hashed_key_123",
///         &user.id,
///         &team.id,
///         Some("My API Key".to_string()),
///     )
///     .await
///     .unwrap();
///
/// assert_eq!(api_key.key_hash, "hashed_key_123");
/// assert_eq!(api_key.name, Some("My API Key".to_string()));
/// assert!(api_key.is_active);
///
/// let fetched = db.get_api_key(&api_key.id).await.unwrap().unwrap();
/// assert_eq!(fetched.id, api_key.id);
/// assert_eq!(fetched.key_hash, "hashed_key_123");
/// # }
/// ```
#[tokio::test]
async fn test_database_create_and_get_api_key() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    let user = db
        .create_user(&team.id, "test@example.com", "admin")
        .await
        .expect("Failed to create user");

    let api_key = db
        .create_api_key(
            "hashed_key_123",
            &user.id,
            &team.id,
            Some("My API Key".to_string()),
        )
        .await
        .expect("Failed to create API key");
    assert_eq!(api_key.key_hash, "hashed_key_123");
    assert_eq!(api_key.name, Some("My API Key".to_string()));
    assert!(api_key.is_active);

    let fetched = db
        .get_api_key(&api_key.id)
        .await
        .expect("Failed to get API key")
        .expect("API key not found");
    assert_eq!(fetched.id, api_key.id);
    assert_eq!(fetched.key_hash, "hashed_key_123");
}

#[tokio::test]
async fn test_database_create_and_get_model_alias() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    let alias = db
        .create_model_alias(&team.id, "gpt-4-fast", "gpt-4-turbo", "openai")
        .await
        .expect("Failed to create model alias");
    assert_eq!(alias.alias, "gpt-4-fast");
    assert_eq!(alias.target_model, "gpt-4-turbo");
    assert_eq!(alias.provider, "openai");

    let fetched = db
        .get_model_alias(&alias.id)
        .await
        .expect("Failed to get model alias")
        .expect("Model alias not found");
    assert_eq!(fetched.id, alias.id);
    assert_eq!(fetched.alias, "gpt-4-fast");
}

#[tokio::test]
async fn test_database_create_and_get_quota() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    let quota = db
        .create_quota(&team.id, 100, 10000)
        .await
        .expect("Failed to create quota");
    assert_eq!(quota.rpm_limit, 100);
    assert_eq!(quota.tpm_limit, 10000);

    let fetched = db
        .get_quota(&team.id)
        .await
        .expect("Failed to get quota")
        .expect("Quota not found");
    assert_eq!(fetched.team_id, team.id);
    assert_eq!(fetched.rpm_limit, 100);
}

#[tokio::test]
async fn test_get_nonexistent_team() {
    let (db, _container) = setup_test_db().await;

    let result = db
        .get_team("00000000-0000-0000-0000-000000000000")
        .await
        .expect("Query failed");
    assert!(result.is_none(), "Should return None for non-existent team");
}

#[tokio::test]
async fn test_get_nonexistent_user() {
    let (db, _container) = setup_test_db().await;

    let result = db
        .get_user("00000000-0000-0000-0000-000000000000")
        .await
        .expect("Query failed");
    assert!(result.is_none(), "Should return None for non-existent user");
}

#[tokio::test]
async fn test_get_nonexistent_api_key() {
    let (db, _container) = setup_test_db().await;

    let result = db
        .get_api_key("00000000-0000-0000-0000-000000000000")
        .await
        .expect("Query failed");
    assert!(
        result.is_none(),
        "Should return None for non-existent API key"
    );
}

#[tokio::test]
async fn test_get_nonexistent_model_alias() {
    let (db, _container) = setup_test_db().await;

    let result = db
        .get_model_alias("00000000-0000-0000-0000-000000000000")
        .await
        .expect("Query failed");
    assert!(
        result.is_none(),
        "Should return None for non-existent model alias"
    );
}

#[tokio::test]
async fn test_get_nonexistent_quota() {
    let (db, _container) = setup_test_db().await;

    let result = db
        .get_quota("00000000-0000-0000-0000-000000000000")
        .await
        .expect("Query failed");
    assert!(
        result.is_none(),
        "Should return None for non-existent quota"
    );
}

/// Verifies that creating two teams with the same name violates the unique-name constraint.
///
/// Attempts to create a team with a name that already exists and asserts that the second
/// creation returns an error.
///
/// # Examples
///
/// ```no_run
/// #[tokio::test]
/// async fn example_duplicate_team_name() {
///     let (db, _container) = setup_test_db().await;
///     db.create_team("Unique Team", 10000).await.unwrap();
///     let result = db.create_team("Unique Team", 20000).await;
///     assert!(result.is_err());
/// }
/// ```
#[tokio::test]
async fn test_duplicate_team_name() {
    let (db, _container) = setup_test_db().await;

    db.create_team("Unique Team", 10000)
        .await
        .expect("Failed to create first team");

    let result = db.create_team("Unique Team", 20000).await;
    assert!(result.is_err(), "Should fail on duplicate team name");
}

#[tokio::test]
async fn test_duplicate_user_email() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    db.create_user(&team.id, "unique@example.com", "admin")
        .await
        .expect("Failed to create first user");

    let result = db
        .create_user(&team.id, "unique@example.com", "member")
        .await;
    assert!(result.is_err(), "Should fail on duplicate user email");
}

#[tokio::test]
async fn test_duplicate_api_key_hash() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    let user = db
        .create_user(&team.id, "test@example.com", "admin")
        .await
        .expect("Failed to create user");

    db.create_api_key("unique_hash", &user.id, &team.id, None)
        .await
        .expect("Failed to create first API key");

    let result = db
        .create_api_key("unique_hash", &user.id, &team.id, None)
        .await;
    assert!(result.is_err(), "Should fail on duplicate API key hash");
}

#[tokio::test]
async fn test_duplicate_model_alias_per_team() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    db.create_model_alias(&team.id, "gpt-4", "gpt-4-turbo", "openai")
        .await
        .expect("Failed to create first alias");

    let result = db
        .create_model_alias(&team.id, "gpt-4", "gpt-4o", "openai")
        .await;
    assert!(
        result.is_err(),
        "Should fail on duplicate model alias per team"
    );
}

#[tokio::test]
async fn test_duplicate_quota_per_team() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    db.create_quota(&team.id, 100, 10000)
        .await
        .expect("Failed to create first quota");

    let result = db.create_quota(&team.id, 200, 20000).await;
    assert!(result.is_err(), "Should fail on duplicate quota per team");
}

#[tokio::test]
async fn test_invalid_uuid_format() {
    let (db, _container) = setup_test_db().await;

    let result = db.get_team("not-a-uuid").await;
    assert!(result.is_err(), "Should fail on invalid UUID format");
}

#[tokio::test]
async fn test_create_user_invalid_team_fk() {
    let (db, _container) = setup_test_db().await;

    let result = db
        .create_user(
            "00000000-0000-0000-0000-000000000000",
            "test@example.com",
            "admin",
        )
        .await;
    assert!(result.is_err(), "Should fail on invalid team foreign key");
}

#[tokio::test]
async fn test_create_api_key_invalid_user_fk() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    let result = db
        .create_api_key(
            "hash123",
            "00000000-0000-0000-0000-000000000000",
            &team.id,
            None,
        )
        .await;
    assert!(result.is_err(), "Should fail on invalid user foreign key");
}

#[tokio::test]
async fn test_create_api_key_invalid_team_fk() {
    let (db, _container) = setup_test_db().await;

    let team = db
        .create_team("Test Team", 10000)
        .await
        .expect("Failed to create team");

    let user = db
        .create_user(&team.id, "test@example.com", "admin")
        .await
        .expect("Failed to create user");

    let result = db
        .create_api_key(
            "hash123",
            &user.id,
            "00000000-0000-0000-0000-000000000000",
            None,
        )
        .await;
    assert!(result.is_err(), "Should fail on invalid team foreign key");
}

#[tokio::test]
async fn test_create_model_alias_invalid_team_fk() {
    let (db, _container) = setup_test_db().await;

    let result = db
        .create_model_alias(
            "00000000-0000-0000-0000-000000000000",
            "gpt-4",
            "gpt-4-turbo",
            "openai",
        )
        .await;
    assert!(result.is_err(), "Should fail on invalid team foreign key");
}

#[tokio::test]
async fn test_create_quota_invalid_team_fk() {
    let (db, _container) = setup_test_db().await;

    let result = db
        .create_quota("00000000-0000-0000-0000-000000000000", 100, 10000)
        .await;
    assert!(result.is_err(), "Should fail on invalid team foreign key");
}