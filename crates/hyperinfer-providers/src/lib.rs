#![allow(clippy::double_must_use)]

pub mod provider_trait;
pub mod registry;

#[cfg(feature = "anthropic")]
pub mod anthropic;
#[cfg(feature = "gemini")]
pub mod gemini;
#[cfg(feature = "openai")]
pub mod openai;

pub use provider_trait::LlmProvider;
pub use registry::ProviderRegistry;

pub fn drain_lines(raw_buf: &mut Vec<u8>, lines: &mut Vec<String>) {
    if raw_buf.is_empty() {
        return;
    }

    // Determine how many bytes are valid UTF-8
    let valid_up_to = match std::str::from_utf8(raw_buf) {
        Ok(_) => raw_buf.len(),
        Err(e) => e.valid_up_to(),
    };

    // Process all complete lines within the valid UTF-8 range
    let mut consumed = 0;
    let mut i = 0;
    while i < valid_up_to {
        if raw_buf[i] == b'\n' {
            // Found a newline - extract the line
            let line_bytes = &raw_buf[consumed..i];
            let line_bytes = line_bytes.strip_suffix(b"\r").unwrap_or(line_bytes);
            lines.push(String::from_utf8_lossy(line_bytes).into_owned());
            consumed = i + 1;
        }
        i += 1;
    }

    // Drain all consumed bytes
    if consumed > 0 {
        raw_buf.drain(..consumed);
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

    #[cfg(feature = "gemini")]
    {
        if let Ok(provider) = gemini::GeminiProvider::new() {
            if !registry.contains(provider.name()) {
                registry.register(provider);
            }
        }
        if let Ok(provider) = gemini::GeminiProvider::new_antigravity() {
            if !registry.contains(provider.name()) {
                registry.register(provider);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed_chunks(chunks: &[&[u8]]) -> (Vec<String>, Vec<u8>) {
        let mut raw_buf: Vec<u8> = Vec::new();
        let mut all_lines: Vec<String> = Vec::new();
        for chunk in chunks {
            raw_buf.extend_from_slice(chunk);
            drain_lines(&mut raw_buf, &mut all_lines);
        }
        (all_lines, raw_buf)
    }

    #[test]
    fn test_drain_lines_single_chunk() {
        let (lines, remainder) = feed_chunks(&[b"data: hello\ndata: world\n"]);
        assert_eq!(lines, vec!["data: hello", "data: world"]);
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_crlf_endings() {
        let (lines, remainder) = feed_chunks(&[b"data: hello\r\ndata: world\r\n"]);
        assert_eq!(lines, vec!["data: hello", "data: world"]);
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_incomplete_line_buffered() {
        let (lines, remainder) = feed_chunks(&[b"data: hello\n", b"data: partial"]);
        assert_eq!(lines, vec!["data: hello"]);
        assert_eq!(remainder, b"data: partial");
    }

    #[test]
    fn test_drain_lines_multibyte_split_across_chunks() {
        let chunk1: &[u8] = b"data: caf\xc3";
        let chunk2: &[u8] = b"\xa9\ndata: done\n";
        let (lines, remainder) = feed_chunks(&[chunk1, chunk2]);
        assert_eq!(lines[0], "data: café");
        assert_eq!(lines[1], "data: done");
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_three_byte_split_across_three_chunks() {
        let chunk1: &[u8] = b"data: \xe4";
        let chunk2: &[u8] = b"\xb8";
        let chunk3: &[u8] = b"\xad\n";
        let (lines, remainder) = feed_chunks(&[chunk1, chunk2, chunk3]);
        assert_eq!(lines, vec!["data: 中"]);
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_empty_lines_preserved() {
        let (lines, _) = feed_chunks(&[b"data: hello\n\ndata: world\n"]);
        assert_eq!(lines, vec!["data: hello", "", "data: world"]);
    }

    #[test]
    fn test_drain_lines_no_newline_nothing_emitted() {
        let (lines, remainder) = feed_chunks(&[b"data: no newline yet"]);
        assert!(lines.is_empty());
        assert_eq!(remainder, b"data: no newline yet");
    }

    #[test]
    fn test_drain_lines_utf8_invalid_bytes_preserved() {
        let chunk1: &[u8] = b"data: \xc3";
        let chunk2: &[u8] = b"\xa9\n";
        let (lines, remainder) = feed_chunks(&[chunk1, chunk2]);
        assert_eq!(lines, vec!["data: é"]);
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_multiple_incomplete_chunks() {
        let chunk1: &[u8] = b"data: \xe4\xb8";
        let chunk2: &[u8] = b"\xad";
        let chunk3: &[u8] = b"\ndata: done\n";
        let (lines, remainder) = feed_chunks(&[chunk1, chunk2, chunk3]);
        assert_eq!(lines, vec!["data: 中", "data: done"]);
        assert!(remainder.is_empty());
    }

    #[test]
    fn test_drain_lines_mixed_crlf_and_lf() {
        let (lines, _) = feed_chunks(&[b"line1\r\nline2\nline3\r\n"]);
        assert_eq!(lines, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_drain_lines_multiple_lines_with_split_multibyte_at_end() {
        // Multiple complete lines in chunk 1, with a split multi-byte UTF-8 char at the end
        let chunk1: &[u8] = b"data: line1\ndata: line2\ndata: caf\xc3";
        let chunk2: &[u8] = b"\xa9\ndata: line4\n";
        let (lines, remainder) = feed_chunks(&[chunk1, chunk2]);
        assert_eq!(
            lines,
            vec!["data: line1", "data: line2", "data: café", "data: line4"]
        );
        assert!(remainder.is_empty());
    }
}
