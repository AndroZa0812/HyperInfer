//! Composable gateway plugin middleware trait and pipeline.
//!
//! Plugins are registered in order and executed as onion-style middleware:
//! pre-request hooks run forward, post-response hooks run in reverse.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::types::{ChatRequest, ChatResponse};

/// Context passed to each plugin hook.
#[derive(Debug, Clone, Default)]
pub struct PluginContext {
    pub request_id: String,
    pub team_id: Option<String>,
    pub api_key_id: Option<String>,
    pub model: String,
    pub is_fallback: bool,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Action a plugin can return from a pre-request hook.
#[derive(Debug, Clone)]
pub enum PluginAction {
    Continue,
    ShortCircuit { status_code: u16, message: String },
    Respond(ChatResponse),
}

/// Gateway plugin middleware trait.
#[async_trait]
pub trait GatewayPlugin: Send + Sync {
    fn name(&self) -> &str;

    async fn pre_request(&self, _ctx: &mut PluginContext, _req: &mut ChatRequest) -> PluginAction {
        PluginAction::Continue
    }

    async fn post_response(&self, _ctx: &mut PluginContext, _resp: &mut ChatResponse) {}

    async fn on_error(&self, _ctx: &mut PluginContext, _err: &str) {}
}

/// Ordered pipeline of gateway plugins.
#[derive(Clone, Default)]
pub struct PluginPipeline {
    plugins: Vec<Arc<dyn GatewayPlugin>>,
    redaction: Option<Arc<crate::redaction::RedactionPlugin>>,
}

impl PluginPipeline {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            redaction: None,
        }
    }

    pub fn add(&mut self, plugin: Arc<dyn GatewayPlugin>) {
        self.plugins.push(plugin);
    }

    pub fn add_redaction(&mut self, plugin: Arc<crate::redaction::RedactionPlugin>) {
        self.redaction = Some(plugin.clone());
        self.plugins.push(plugin);
    }

    pub fn redaction(&self) -> Option<Arc<crate::redaction::RedactionPlugin>> {
        self.redaction.clone()
    }

    pub fn with_plugin(mut self, plugin: Arc<dyn GatewayPlugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn plugin_names(&self) -> Vec<String> {
        self.plugins.iter().map(|p| p.name().to_string()).collect()
    }

    /// Run pre-request hooks in registration order.
    /// Stops immediately on ShortCircuit or Respond.
    pub async fn run_pre_request(
        &self,
        ctx: &mut PluginContext,
        req: &mut ChatRequest,
    ) -> PluginAction {
        for plugin in &self.plugins {
            let action = plugin.pre_request(ctx, req).await;
            if !matches!(action, PluginAction::Continue) {
                return action;
            }
        }
        PluginAction::Continue
    }

    /// Run post-response hooks in reverse registration order (onion style).
    pub async fn run_post_response(&self, ctx: &mut PluginContext, resp: &mut ChatResponse) {
        for plugin in self.plugins.iter().rev() {
            plugin.post_response(ctx, resp).await;
        }
    }

    /// Run on_error hooks in reverse registration order.
    pub async fn run_on_error(&self, ctx: &mut PluginContext, err: &str) {
        for plugin in self.plugins.iter().rev() {
            plugin.on_error(ctx, err).await;
        }
    }
}

#[cfg(test)]
include!("plugin_tests.rs");
