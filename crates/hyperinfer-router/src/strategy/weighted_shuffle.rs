use super::{RoutingContext, RoutingState, RoutingStrategy};
use crate::deployment::Deployment;
use crate::error::RoutingError;
use async_trait::async_trait;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct WeightedShuffle;

impl WeightedShuffle {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WeightedShuffle {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl RoutingStrategy for WeightedShuffle {
    fn name(&self) -> &str {
        "weighted-shuffle"
    }

    async fn select<'a>(
        &self,
        _model: &str,
        candidates: &'a [Arc<Deployment>],
        _state: &dyn RoutingState,
        _request: &RoutingContext,
    ) -> Result<&'a Arc<Deployment>, RoutingError> {
        candidates
            .first()
            .ok_or_else(|| RoutingError::NoDeployments("empty candidates".into()))
    }
}
