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
    ) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>> {
        let redis_url = self.client.get_connection_info().addr().to_string();
        
        let handle = tokio::spawn(async move {
            let mut backoff = 1u64;
            
            loop {
                let result = async {
                    let client = Client::open(redis_url.as_str())?;
                    let mut pubsub = client.get_async_pubsub().await?;
                    pubsub.subscribe(CONFIG_CHANNEL).await?;
                    
                    info!("Subscribed to Redis config updates channel: {}", CONFIG_CHANNEL);
                    
                    let mut stream = pubsub.on_message();
                    
                    while let Some(msg) = stream.next().await {
                        let payload_str = match msg.get_payload::<String>() {
                            Ok(p) => p,
                            Err(e) => {
                                error!("Failed to get message payload: {}", e);
                                continue;
                            }
                        };
                        
                        let new_config = match serde_json::from_str::<ConfigUpdate>(&payload_str) {
                            Ok(update) => update.config,
                            Err(e) => {
                                error!("Failed to parse config update: {}", e);
                                continue;
                            }
                        };
                        
                        {
                            let mut cfg = config.write().await;
                            *cfg = new_config;
                            info!("Config updated via Pub/Sub");
                        }
                    }
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                }.await;
                
                if let Err(e) = result {
                    error!("Config subscription error: {}, reconnecting in {}s", e, backoff);
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(60);
                } else {
                    error!("Config updates subscription stream ended unexpectedly");
                    break;
                }
            }
        });
        
        Ok(handle)
    }

    pub async fn subscribe_to_policy_updates(
        &self,
        callback: impl Fn(PolicyUpdate) + Send + Sync + 'static,
    ) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error + Send + Sync>> {
        let redis_url = self.client.get_connection_info().addr().to_string();
        
        let handle = tokio::spawn(async move {
            let mut backoff = 1u64;
            
            loop {
                let result = async {
                    let client = Client::open(redis_url.as_str())?;
                    let mut pubsub = client.get_async_pubsub().await?;
                    pubsub.subscribe("hyperinfer:policy_updates").await?;
                    
                    info!("Subscribed to Redis policy updates channel");
                    
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
                    Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
                }.await;
                
                if let Err(e) = result {
                    error!("Policy subscription error: {}, reconnecting in {}s", e, backoff);
                    tokio::time::sleep(tokio::time::Duration::from_secs(backoff)).await;
                    backoff = (backoff * 2).min(60);
                } else {
                    error!("Policy updates subscription stream ended unexpectedly");
                    break;
                }
            }
        });
        
        Ok(handle)
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
        
        // Store config first so it's available when subscribers receive notification
        let config_bytes = serde_json::to_vec(config)?;
        
        redis::cmd("SET")
            .arg(CONFIG_KEY)
            .arg(config_bytes)
            .query_async::<()>(&mut conn)
            .await?;
        
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
