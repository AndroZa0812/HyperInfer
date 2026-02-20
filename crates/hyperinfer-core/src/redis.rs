//! Redis utilities for HyperInfer
//!
//! Provides functionality for Redis-based configuration and policy updates.

use futures_util::stream::StreamExt;
use redis::aio::ConnectionManager;
use redis::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, error};

use crate::types::Config;

pub const CONFIG_CHANNEL: &str = "hyperinfer:config_updates";
pub const CONFIG_KEY: &str = "hyperinfer:config";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigUpdate {
    pub config: Config,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyUpdate {
    pub key: String,
    pub action: PolicyAction,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyAction {
    Revoke,
    Update,
}

pub struct ConfigManager {
    client: Client,
    manager: ConnectionManager,
}

impl ConfigManager {
    pub async fn new(redis_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Client::open(redis_url)?;
        let manager = ConnectionManager::new(client.clone()).await?;
        Ok(Self { client, manager })
    }

    pub async fn subscribe_to_config_updates(
        &self,
        config: Arc<RwLock<Config>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut pubsub = self.client.get_async_pubsub().await?;
        
        pubsub.subscribe(CONFIG_CHANNEL).await?;
        
        info!("Subscribed to Redis config updates channel: {}", CONFIG_CHANNEL);
        
        let config_clone = config.clone();
        
        tokio::spawn(async move {
            let mut stream = pubsub.on_message();
            
            while let Some(msg) = stream.next().await {
                let payload = match msg.get_payload::<String>() {
                    Ok(p) => p,
                    Err(e) => {
                        error!("Failed to get message payload: {}", e);
                        continue;
                    }
                };
                
                match serde_json::from_str::<ConfigUpdate>(&payload) {
                    Ok(update) => {
                        let mut cfg = config_clone.write().await;
                        *cfg = update.config;
                        info!("Config updated via Pub/Sub");
                    }
                    Err(e) => {
                        error!("Failed to parse config update: {}", e);
                    }
                }
            }
        });
        
        Ok(())
    }

    pub async fn subscribe_to_policy_updates(
        &self,
        callback: impl Fn(PolicyUpdate) + Send + 'static,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut pubsub = self.client.get_async_pubsub().await?;
        
        pubsub.subscribe("hyperinfer:policy_updates").await?;
        
        info!("Subscribed to Redis policy updates channel");
        
        tokio::spawn(async move {
            let mut stream = pubsub.on_message();
            
            while let Some(msg) = stream.next().await {
                let payload = match msg.get_payload::<String>() {
                    Ok(p) => p,
                    Err(e) => {
                        error!("Failed to get policy message payload: {}", e);
                        continue;
                    }
                };
                
                match serde_json::from_str::<PolicyUpdate>(&payload) {
                    Ok(update) => callback(update),
                    Err(e) => error!("Failed to parse policy update: {}", e),
                }
            }
        });
        
        Ok(())
    }

    pub async fn fetch_config(&self) -> Result<Config, Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.manager.clone();
        
        let data: Option<Vec<u8>> = redis::cmd("GET")
            .arg(CONFIG_KEY)
            .query_async(&mut conn)
            .await?;
        
        match data {
            Some(bytes) => {
                let config: Config = serde_json::from_slice(&bytes)?;
                Ok(config)
            }
            None => {
                Ok(Config {
                    api_keys: std::collections::HashMap::new(),
                    routing_rules: Vec::new(),
                    quotas: std::collections::HashMap::new(),
                    model_aliases: std::collections::HashMap::new(),
                })
            }
        }
    }

    pub async fn publish_config_update(&self, config: &Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.manager.clone();
        
        let update = ConfigUpdate {
            config: config.clone(),
        };
        
        let payload = serde_json::to_string(&update)?;
        
        redis::cmd("PUBLISH")
            .arg(CONFIG_CHANNEL)
            .arg(&payload)
            .query_async::<()>(&mut conn)
            .await?;
        
        info!("Published config update to channel: {}", CONFIG_CHANNEL);
        
        let config_bytes = serde_json::to_vec(config)?;
        
        redis::cmd("SET")
            .arg(CONFIG_KEY)
            .arg(config_bytes)
            .query_async::<()>(&mut conn)
            .await?;
        
        Ok(())
    }

    pub async fn publish_policy_update(&self, update: &PolicyUpdate) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.manager.clone();
        
        let payload = serde_json::to_string(update)?;
        
        redis::cmd("PUBLISH")
            .arg("hyperinfer:policy_updates")
            .arg(&payload)
            .query_async::<()>(&mut conn)
            .await?;
        
        info!("Published policy update: {:?}", update.action);
        
        Ok(())
    }
}
