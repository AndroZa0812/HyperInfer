use hyperinfer_core::Database;
use hyperinfer_server::SqlxDb;
use sqlx::postgres::PgPoolOptions;
use testcontainers::{runners::AsyncRunner, ContainerAsync};
use testcontainers_modules::postgres::Postgres;

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
