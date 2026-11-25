use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use hyperinfer_core::error::ConfigError;
use hyperinfer_core::redis::PolicyUpdate;
use hyperinfer_core::traits::ConfigStore;
use hyperinfer_core::types::Config;

/// In-memory config store for standalone mode (no Redis).
#[derive(Clone)]
pub struct InMemoryConfigStore {
    config: Arc<RwLock<Config>>,
}

impl InMemoryConfigStore {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ConfigStore for InMemoryConfigStore {
    async fn fetch_config(&self) -> Result<Config, ConfigError> {
        Ok(self.config.read().await.clone())
    }

    async fn publish_config_update(&self, config: &Config) -> Result<(), ConfigError> {
        *self.config.write().await = config.clone();
        Ok(())
    }

    async fn publish_policy_update(&self, _update: &PolicyUpdate) -> Result<(), ConfigError> {
        Ok(())
    }

    async fn ping(&self) -> Result<(), ConfigError> {
        Ok(())
    }
}
