pub struct Telemetry {
    redis_url: String,
}

impl Telemetry {
    pub async fn new(redis_url: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            redis_url: redis_url.to_string(),
        })
    }

    pub async fn record(
        &self,
        _key: &str,
        _model: &str,
        _response_time_ms: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::debug!("Recording telemetry");
        Ok(())
    }
}
