#![allow(clippy::double_must_use)]

pub mod auth;
pub mod db;
pub mod frontend;
pub mod mcp;
pub mod mcp_gateway;
pub mod mutators;
pub mod protocols;
pub mod proxy;
pub mod routes;
pub mod safe_resolver;
pub mod seeding;
pub mod state;

pub use db::{RedisConfigStore, SqlxDb};
pub use routes::build_router;
pub use state::AppState;
