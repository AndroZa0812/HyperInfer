use async_trait::async_trait;

use crate::error::ConfigError;
use crate::redis::PolicyUpdate;
use crate::types::Config;

#[async_trait]
pub trait ConfigStore: Clone + Send + Sync + 'static {
    async fn fetch_config(&self) -> Result<Config, ConfigError>;
    async fn publish_config_update(&self, config: &Config) -> Result<(), ConfigError>;
    async fn publish_policy_update(&self, update: &PolicyUpdate) -> Result<(), ConfigError>;
}
