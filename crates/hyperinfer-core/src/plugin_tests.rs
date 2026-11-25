#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChatMessage, MessageRole};

    #[derive(Default)]
    struct OrderPlugin {
        name: String,
        pre_order: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        post_order: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        err_order: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl OrderPlugin {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                pre_order: Default::default(),
                post_order: Default::default(),
                err_order: Default::default(),
            }
        }
    }

    #[async_trait]
    impl GatewayPlugin for OrderPlugin {
        fn name(&self) -> &str {
            &self.name
        }

        async fn pre_request(
            &self,
            _ctx: &mut PluginContext,
            _req: &mut ChatRequest,
        ) -> PluginAction {
            self.pre_order.lock().unwrap().push(self.name.clone());
            PluginAction::Continue
        }

        async fn post_response(&self, _ctx: &mut PluginContext, _resp: &mut ChatResponse) {
            self.post_order.lock().unwrap().push(self.name.clone());
        }

        async fn on_error(&self, _ctx: &mut PluginContext, _err: &str) {
            self.err_order.lock().unwrap().push(self.name.clone());
        }
    }

    fn make_req() -> ChatRequest {
        ChatRequest {
            model: "test".to_string(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
                name: None,
                tool_calls: None,
                tool_call_id: None,
                content_parts: None,
            }],
            temperature: None,
            max_tokens: None,
            stream: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            stop: None,
        }
    }

    fn make_resp() -> ChatResponse {
        ChatResponse {
            id: "test".to_string(),
            model: "test".to_string(),
            choices: vec![],
            usage: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_pre_forward_post_reverse() {
        let p1 = Arc::new(OrderPlugin::new("p1"));
        let p2 = Arc::new(OrderPlugin::new("p2"));
        let p3 = Arc::new(OrderPlugin::new("p3"));

        let pipeline = PluginPipeline::new()
            .with_plugin(p1.clone())
            .with_plugin(p2.clone())
            .with_plugin(p3.clone());

        let mut ctx = PluginContext::default();
        let mut req = make_req();
        let action = pipeline.run_pre_request(&mut ctx, &mut req).await;
        assert!(matches!(action, PluginAction::Continue));

        assert_eq!(p1.pre_order.lock().unwrap().clone(), vec!["p1"]);
        assert_eq!(p2.pre_order.lock().unwrap().clone(), vec!["p2"]);
        assert_eq!(p3.pre_order.lock().unwrap().clone(), vec!["p3"]);

        let mut resp = make_resp();
        pipeline.run_post_response(&mut ctx, &mut resp).await;

        assert_eq!(p1.post_order.lock().unwrap().clone(), vec!["p1"]);
        assert_eq!(p2.post_order.lock().unwrap().clone(), vec!["p2"]);
        assert_eq!(p3.post_order.lock().unwrap().clone(), vec!["p3"]);
    }

    #[tokio::test]
    async fn test_short_circuit() {
        #[derive(Default)]
        struct ShortCircuitPlugin;

        #[async_trait]
        impl GatewayPlugin for ShortCircuitPlugin {
            fn name(&self) -> &str {
                "short"
            }

            async fn pre_request(
                &self,
                _ctx: &mut PluginContext,
                _req: &mut ChatRequest,
            ) -> PluginAction {
                PluginAction::ShortCircuit {
                    status_code: 403,
                    message: "blocked".to_string(),
                }
            }
        }

        let p1 = Arc::new(OrderPlugin::new("p1"));
        let sc = Arc::new(ShortCircuitPlugin);
        let p2 = Arc::new(OrderPlugin::new("p2"));

        let pipeline = PluginPipeline::new()
            .with_plugin(p1.clone())
            .with_plugin(sc)
            .with_plugin(p2.clone());

        let mut ctx = PluginContext::default();
        let mut req = make_req();
        let action = pipeline.run_pre_request(&mut ctx, &mut req).await;

        match action {
            PluginAction::ShortCircuit { status_code, message } => {
                assert_eq!(status_code, 403);
                assert_eq!(message, "blocked");
            }
            _ => panic!("expected ShortCircuit"),
        }

        assert_eq!(p1.pre_order.lock().unwrap().clone(), vec!["p1"]);
        assert!(p2.pre_order.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_on_error_all_plugins() {
        let p1 = Arc::new(OrderPlugin::new("p1"));
        let p2 = Arc::new(OrderPlugin::new("p2"));

        let pipeline = PluginPipeline::new()
            .with_plugin(p1.clone())
            .with_plugin(p2.clone());

        let mut ctx = PluginContext::default();
        pipeline.run_on_error(&mut ctx, "test error").await;

        assert_eq!(p1.err_order.lock().unwrap().clone(), vec!["p1"]);
        assert_eq!(p2.err_order.lock().unwrap().clone(), vec!["p2"]);
    }
}
