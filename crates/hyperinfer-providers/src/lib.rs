pub mod provider_trait;
pub mod registry;

#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "openai")]
pub mod openai;

pub use provider_trait::LlmProvider;
pub use registry::ProviderRegistry;

pub fn drain_lines(raw_buf: &mut Vec<u8>, lines: &mut Vec<String>) {
    while let Some(pos) = raw_buf.iter().position(|&b| b == b'\n') {
        let line_bytes = &raw_buf[..pos];
        let line_bytes = line_bytes.strip_suffix(b"\r").unwrap_or(line_bytes);
        lines.push(String::from_utf8_lossy(line_bytes).into_owned());
        raw_buf.drain(..=pos);
    }
}

pub fn init_default_registry(registry: &ProviderRegistry) {
    #[cfg(feature = "openai")]
    {
        if let Ok(provider) = openai::OpenAiProvider::new() {
            if !registry.contains(provider.name()) {
                registry.register(provider);
            }
        }
    }

    #[cfg(feature = "anthropic")]
    {
        if let Ok(provider) = anthropic::AnthropicProvider::new() {
            if !registry.contains(provider.name()) {
                registry.register(provider);
            }
        }
    }
}
