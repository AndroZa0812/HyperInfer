pub mod provider_trait;
pub mod registry;

#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "openai")]
pub mod openai;

pub use provider_trait::LlmProvider;
pub use registry::ProviderRegistry;

pub fn drain_lines(raw_buf: &mut Vec<u8>, lines: &mut Vec<String>) {
    if raw_buf.is_empty() {
        return;
    }

    match std::str::from_utf8(raw_buf) {
        Ok(s) => {
            let mut rest = s;
            while let Some(pos) = rest.find('\n') {
                let (line, next_rest) = rest.split_at(pos);
                let line = line.strip_suffix('\r').unwrap_or(line);
                lines.push(line.to_string());
                rest = &next_rest[1..];
            }
            if rest.is_empty() {
                raw_buf.clear();
            } else {
                *raw_buf = rest.as_bytes().to_vec();
            }
        }
        Err(e) => {
            let valid_up_to = e.valid_up_to();
            let valid_prefix = &raw_buf[..valid_up_to];

            if let Some(pos) = valid_prefix.iter().rposition(|&b| b == b'\n') {
                let line_bytes = &valid_prefix[..pos];
                let line_bytes = line_bytes.strip_suffix(b"\r").unwrap_or(line_bytes);
                lines.push(String::from_utf8_lossy(line_bytes).into_owned());
                raw_buf.drain(..=pos + 1);
            }
        }
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
