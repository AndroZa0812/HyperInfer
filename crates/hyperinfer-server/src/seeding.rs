use crate::db::hash_password;
use hyperinfer_core::{Database, DbError};
use std::env;

/// Run seeding logic on server startup.
///
/// Per the spec:
/// - Check if any admin users exist
/// - If no admin users and INITIAL_ADMIN_EMAIL/PASSWORD are set, create initial admin
/// - Password must be at least 8 characters
pub async fn run_seeding<D: Database>(db: &D) -> Result<(), DbError> {
    // Check if any admin users already exist
    match db.count_users_by_role("admin").await? {
        count if count > 0 => {
            tracing::info!(
                "Admin users exist (count: {}), ignoring INITIAL_ADMIN_* env vars",
                count
            );
            return Ok(());
        }
        _ => {}
    }

    // Get initial admin credentials from environment
    let admin_email = match env::var("INITIAL_ADMIN_EMAIL") {
        Ok(email) if !email.is_empty() => email,
        Ok(_) => {
            tracing::warn!("INITIAL_ADMIN_EMAIL is set but empty, skipping seeding");
            return Ok(());
        }
        Err(_) => {
            tracing::info!("INITIAL_ADMIN_EMAIL not set, skipping seeding");
            return Ok(());
        }
    };

    let admin_password = match env::var("INITIAL_ADMIN_PASSWORD") {
        Ok(password) if !password.is_empty() => password,
        Ok(_) => {
            return Err(DbError::ValidationError(
                "INITIAL_ADMIN_PASSWORD must be non-empty".to_string(),
            ));
        }
        Err(_) => {
            return Err(DbError::ValidationError(
                "INITIAL_ADMIN_PASSWORD must be set when INITIAL_ADMIN_EMAIL is set".to_string(),
            ));
        }
    };

    // Validate password length (min 8 characters per spec)
    if admin_password.len() < 8 {
        return Err(DbError::ValidationError(
            "INITIAL_ADMIN_PASSWORD must be at least 8 characters".to_string(),
        ));
    }

    // Check if user already exists by email
    if let Some(_existing_user) = db.get_user_by_email(&admin_email).await? {
        tracing::info!(
            "User with email '{}' already exists, skipping seeding",
            admin_email
        );
        return Ok(());
    }

    tracing::info!("Seeding initial admin user: {}", admin_email);

    // Create default team
    let team_name = env::var("SEED_TEAM_NAME").unwrap_or_else(|_| "Default Team".to_string());
    let team = db.create_team(&team_name, 10000).await?;

    // Hash password and create admin user
    let password_hash = hash_password(&admin_password)?;

    db.create_user(&team.id, &admin_email, "admin", Some(&password_hash))
        .await?;

    tracing::info!(
        "Seeding complete: Created team '{}' and admin user '{}'",
        team_name,
        admin_email
    );

    Ok(())
}
