//! Reversible redaction vault, streaming re-hydrator, and smart redaction
//! connectors (Microsoft Presidio Analyzer & local/trusted LLM pre-pass).

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::types::{ChatMessage, ContentPart};

/// Metadata key used to store the per-request [`RedactionVault`] between
/// `pre_request` and `post_response`.
pub const REDACTION_VAULT_METADATA_KEY: &str = "redaction_vault";

/// Metadata key used to bypass redaction for internal requests (recursion guard).
pub const SKIP_REDACTION_METADATA_KEY: &str = "skip_redaction";

/// HTTP header attached to outbound LLM redaction calls to prevent recursion
/// if the configured redaction endpoint is routed back through HyperInfer.
pub const SKIP_REDACTION_HEADER: &str = "X-HyperInfer-Skip-Redaction";

/// Placeholder prefix used by reversible redaction tokens.
const PLACEHOLDER_PREFIX: &str = "[REDACTED_";

/// Format used for reversible redaction placeholders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceholderFormat {
    /// Sequential indexed placeholders: `[REDACTED_API_KEY_1]`, `[REDACTED_EMAIL_2]`.
    #[default]
    Indexed,
    /// Random 8-char hex UUID suffix: `[REDACTED_API_KEY_a1b2c3d4]`.
    Uuid,
}

/// Backend used for smart (semantic) redaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartRedactionBackend {
    /// Microsoft Presidio Analyzer HTTP sidecar (`POST /analyze`).
    #[default]
    Presidio,
    /// Local small LLM or trusted OpenAI-compatible provider (`POST /chat/completions`).
    Llm,
}

/// Behavior when the smart redaction backend is unreachable, times out, or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmartRedactionFailMode {
    /// Log a warning and fall back to the deterministic regex-redacted text.
    #[default]
    FallbackRegex,
    /// Short-circuit the request with HTTP 503 Service Unavailable.
    FailClosed,
}

/// Configuration for smart PII/secret redaction via Presidio or a local/trusted LLM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SmartRedactionConfig {
    pub enabled: bool,
    pub backend: SmartRedactionBackend,
    pub presidio_url: String,
    pub presidio_language: String,
    pub presidio_entities: Vec<String>,
    pub presidio_score_threshold: f64,
    pub llm_base_url: String,
    pub llm_model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_api_key: Option<String>,
    pub llm_categories: Vec<String>,
    pub timeout_ms: u64,
    pub fail_mode: SmartRedactionFailMode,
}

impl Default for SmartRedactionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            backend: SmartRedactionBackend::Presidio,
            presidio_url: "http://127.0.0.1:5001".to_string(),
            presidio_language: "en".to_string(),
            presidio_entities: vec![
                "PERSON".to_string(),
                "LOCATION".to_string(),
                "PHONE_NUMBER".to_string(),
                "CREDIT_CARD".to_string(),
                "IBAN_CODE".to_string(),
                "US_SSN".to_string(),
                "EMAIL_ADDRESS".to_string(),
            ],
            presidio_score_threshold: 0.5,
            llm_base_url: "http://127.0.0.1:11434/v1".to_string(),
            llm_model: "qwen2.5:3b".to_string(),
            llm_api_key: None,
            llm_categories: vec![
                "pii".to_string(),
                "secrets".to_string(),
                "financial".to_string(),
                "internal_names".to_string(),
            ],
            timeout_ms: 3000,
            fail_mode: SmartRedactionFailMode::FallbackRegex,
        }
    }
}

/// Normalize a category label to uppercase ASCII alphanumeric + underscores.
pub(crate) fn normalize_category(category: &str) -> String {
    let normalized: String = category
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = normalized.trim_matches('_');
    if trimmed.is_empty() {
        "SECRET".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Per-request reversible redaction vault mapping placeholders back to original values.
///
/// The vault lives strictly in memory for the duration of a single request and is
/// never written to logs, traces, or caches.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RedactionVault {
    placeholder_to_original: HashMap<String, String>,
    original_to_placeholder: HashMap<String, String>,
    next_index: usize,
}

impl Default for RedactionVault {
    fn default() -> Self {
        Self {
            placeholder_to_original: HashMap::new(),
            original_to_placeholder: HashMap::new(),
            next_index: 1,
        }
    }
}

impl RedactionVault {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tokenize `original` into a deterministic placeholder for this request.
    /// Repeated occurrences of the same value reuse the existing placeholder.
    pub fn tokenize(
        &mut self,
        category: &str,
        original: &str,
        format: PlaceholderFormat,
    ) -> String {
        if let Some(existing) = self.original_to_placeholder.get(original) {
            return existing.clone();
        }

        let cat = normalize_category(category);
        let placeholder = match format {
            PlaceholderFormat::Indexed => {
                let idx = self.next_index;
                self.next_index += 1;
                format!("{}{}_{}]", PLACEHOLDER_PREFIX, cat, idx)
            }
            PlaceholderFormat::Uuid => loop {
                let short = &uuid::Uuid::new_v4().simple().to_string()[..8];
                let candidate = format!("{}{}_{}]", PLACEHOLDER_PREFIX, cat, short);
                if !self.placeholder_to_original.contains_key(&candidate) {
                    break candidate;
                }
            },
        };

        self.placeholder_to_original
            .insert(placeholder.clone(), original.to_string());
        self.original_to_placeholder
            .insert(original.to_string(), placeholder.clone());
        placeholder
    }

    pub fn is_empty(&self) -> bool {
        self.placeholder_to_original.is_empty()
    }

    pub fn mappings(&self) -> &HashMap<String, String> {
        &self.placeholder_to_original
    }

    /// Restore all placeholders present in `input` to their original values.
    pub fn restore_str(&self, input: &str) -> String {
        if self.placeholder_to_original.is_empty() || !input.contains(PLACEHOLDER_PREFIX) {
            return input.to_string();
        }
        // Replace longest placeholders first to avoid prefix collisions.
        let mut pairs: Vec<(&String, &String)> = self.placeholder_to_original.iter().collect();
        pairs.sort_by_key(|pair| std::cmp::Reverse(pair.0.len()));

        let mut out = input.to_string();
        for (placeholder, original) in pairs {
            if out.contains(placeholder.as_str()) {
                out = out.replace(placeholder.as_str(), original.as_str());
            }
        }
        out
    }

    /// Restore placeholders in a chat message (text content, multimodal parts, tool arguments).
    pub fn restore_message(&self, msg: &mut ChatMessage) {
        if self.placeholder_to_original.is_empty() {
            return;
        }
        msg.content = self.restore_str(&msg.content);
        if let Some(parts) = &mut msg.content_parts {
            for part in parts {
                if let ContentPart::Text { text } = part {
                    *text = self.restore_str(text);
                }
            }
        }
        if let Some(calls) = &mut msg.tool_calls {
            for call in calls {
                call.arguments = self.restore_str(&call.arguments);
            }
        }
    }
}

/// Stateful streaming re-hydrator for SSE chat completion deltas.
///
/// Buffers only the minimal trailing suffix that could be an incomplete
/// `[REDACTED_...]` placeholder split across adjacent token chunks.
#[derive(Debug, Clone)]
pub struct StreamRehydrator {
    vault: RedactionVault,
    pending: String,
}

impl StreamRehydrator {
    pub fn new(vault: RedactionVault) -> Self {
        Self {
            vault,
            pending: String::new(),
        }
    }

    /// Push the next streamed text delta and return the safe re-hydrated prefix.
    pub fn push_chunk(&mut self, delta: &str) -> String {
        if self.vault.is_empty() {
            return delta.to_string();
        }
        self.pending.push_str(delta);
        self.pending = self.vault.restore_str(&self.pending);

        let hold_from = find_incomplete_placeholder_start(&self.pending);
        if hold_from == self.pending.len() {
            std::mem::take(&mut self.pending)
        } else {
            let safe = self.pending[..hold_from].to_string();
            self.pending = self.pending[hold_from..].to_string();
            safe
        }
    }

    /// Flush any remaining buffered text at the end of the stream.
    pub fn flush(&mut self) -> String {
        let remaining = std::mem::take(&mut self.pending);
        self.vault.restore_str(&remaining)
    }
}

/// Returns the byte index where a potentially incomplete `[REDACTED_...]` token starts,
/// or `s.len()` if the entire string is safe to emit.
fn find_incomplete_placeholder_start(s: &str) -> usize {
    // Case 1: unclosed "[REDACTED_" present in `s` without a closing ']'.
    if let Some(idx) = s.rfind(PLACEHOLDER_PREFIX) {
        let tail = &s[idx..];
        if !tail.contains(']') {
            return idx;
        }
    }
    // Case 2: `s` ends with a strict prefix of "[REDACTED_" (e.g. "[", "[REDA").
    let max_prefix = (PLACEHOLDER_PREFIX.len() - 1).min(s.len());
    for len in (1..=max_prefix).rev() {
        let start = s.len() - len;
        if s.is_char_boundary(start) && PLACEHOLDER_PREFIX.starts_with(&s[start..]) {
            return start;
        }
    }
    s.len()
}

// ---------------------------------------------------------------------------
// Smart Redaction Backends (Microsoft Presidio & Local/Trusted LLM)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct PresidioAnalyzeRequest<'a> {
    text: &'a str,
    language: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    entities: Vec<String>,
    score_threshold: f64,
}

#[derive(Debug, Deserialize)]
struct PresidioSpan {
    start: usize,
    end: usize,
    entity_type: String,
    score: f64,
}

/// Call Microsoft Presidio Analyzer (`POST {presidio_url}/analyze`) and replace detected spans.
pub(crate) async fn redact_with_presidio(
    client: &reqwest::Client,
    text: &str,
    mut vault: Option<&mut RedactionVault>,
    format: PlaceholderFormat,
    smart: &SmartRedactionConfig,
) -> Result<String, String> {
    let url = format!("{}/analyze", smart.presidio_url.trim_end_matches('/'));
    let payload = PresidioAnalyzeRequest {
        text,
        language: &smart.presidio_language,
        entities: smart.presidio_entities.clone(),
        score_threshold: smart.presidio_score_threshold,
    };

    let timeout = Duration::from_millis(smart.timeout_ms.max(1));
    let payload_str = serde_json::to_string(&payload)
        .map_err(|e| format!("Failed to serialize Presidio request: {}", e))?;
    let response = tokio::time::timeout(
        timeout,
        client
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(payload_str)
            .send(),
    )
    .await
    .map_err(|_| format!("Presidio analyzer timed out after {}ms", smart.timeout_ms))?
    .map_err(|e| format!("Presidio analyzer request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Presidio analyzer returned HTTP {}",
            response.status()
        ));
    }

    let body_text = tokio::time::timeout(timeout, response.text())
        .await
        .map_err(|_| "Presidio response body timed out".to_string())?
        .map_err(|e| format!("Failed to read Presidio response: {}", e))?;
    let mut spans: Vec<PresidioSpan> = serde_json::from_str(&body_text)
        .map_err(|e| format!("Failed to parse Presidio response: {}", e))?;

    // Presidio returns character offsets (Python str indices); convert to UTF-8 byte offsets.
    let char_to_byte: Vec<usize> = text
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(text.len()))
        .collect();
    let char_len = char_to_byte.len() - 1;

    spans.retain(|s| {
        s.score >= smart.presidio_score_threshold && s.start < s.end && s.end <= char_len
    });

    // Resolve overlaps: prefer higher score, then longer span; keep non-overlapping spans.
    spans.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| (b.end - b.start).cmp(&(a.end - a.start)))
    });

    let mut accepted: Vec<PresidioSpan> = Vec::new();
    for candidate in spans {
        let overlaps = accepted
            .iter()
            .any(|kept| candidate.start < kept.end && candidate.end > kept.start);
        if !overlaps {
            accepted.push(candidate);
        }
    }

    // Sort ascending so indexed vault tokens are assigned in natural left-to-right reading order.
    accepted.sort_by_key(|s| s.start);

    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for span in accepted {
        let start_b = char_to_byte[span.start];
        let end_b = char_to_byte[span.end];
        if start_b < cursor || end_b > text.len() {
            continue;
        }
        out.push_str(&text[cursor..start_b]);
        let matched = &text[start_b..end_b];
        let replacement = if let Some(v) = vault.as_deref_mut() {
            v.tokenize(&span.entity_type, matched, format)
        } else {
            format!("[REDACTED_{}]", normalize_category(&span.entity_type))
        };
        out.push_str(&replacement);
        cursor = end_b;
    }
    out.push_str(&text[cursor..]);
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct LlmSpansPayload {
    #[serde(default)]
    spans: Vec<LlmSpan>,
}

#[derive(Debug, Deserialize)]
struct LlmSpan {
    text: String,
    #[serde(default = "default_llm_category")]
    category: String,
}

fn default_llm_category() -> String {
    "PII".to_string()
}

/// Call a local small LLM or trusted OpenAI-compatible endpoint to detect sensitive spans.
pub(crate) async fn redact_with_llm(
    client: &reqwest::Client,
    text: &str,
    mut vault: Option<&mut RedactionVault>,
    format: PlaceholderFormat,
    smart: &SmartRedactionConfig,
) -> Result<String, String> {
    let url = format!(
        "{}/chat/completions",
        smart.llm_base_url.trim_end_matches('/')
    );
    let categories = if smart.llm_categories.is_empty() {
        "pii, secrets".to_string()
    } else {
        smart.llm_categories.join(", ")
    };

    let system_prompt = format!(
        "You are a PII and secret redaction detector. Identify all sensitive substrings in the \
         user text belonging to these categories: {categories}. Ignore already-redacted tokens \
         like [REDACTED_...]. Return ONLY valid JSON with the exact schema: \
         {{\"spans\": [{{\"text\": \"exact substring\", \"category\": \"CATEGORY\"}}]}}. \
         If nothing is sensitive, return {{\"spans\": []}}."
    );

    let body = serde_json::json!({
        "model": smart.llm_model,
        "temperature": 0.0,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": text }
        ]
    });

    let body_str = serde_json::to_string(&body)
        .map_err(|e| format!("Failed to serialize LLM redactor request: {}", e))?;
    let mut req = client
        .post(&url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(SKIP_REDACTION_HEADER, "1")
        .body(body_str);
    if let Some(api_key) = smart
        .llm_api_key
        .as_deref()
        .filter(|k| !k.trim().is_empty())
    {
        req = req.bearer_auth(api_key.trim());
    }

    let timeout = Duration::from_millis(smart.timeout_ms.max(1));
    let response = tokio::time::timeout(timeout, req.send())
        .await
        .map_err(|_| format!("LLM redactor timed out after {}ms", smart.timeout_ms))?
        .map_err(|e| format!("LLM redactor request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("LLM redactor returned HTTP {}", response.status()));
    }

    let resp_text = tokio::time::timeout(timeout, response.text())
        .await
        .map_err(|_| "LLM redactor response body timed out".to_string())?
        .map_err(|e| format!("Failed to read LLM redactor response: {}", e))?;
    let json_resp: serde_json::Value = serde_json::from_str(&resp_text)
        .map_err(|e| format!("Failed to parse LLM redactor HTTP JSON: {}", e))?;

    let content = json_resp
        .pointer("/choices/0/message/content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "LLM redactor response missing choices[0].message.content".to_string())?;

    let cleaned = strip_markdown_json_fence(content);
    let parsed: LlmSpansPayload = serde_json::from_str(cleaned)
        .map_err(|e| format!("Failed to parse LLM redactor spans JSON: {}", e))?;

    let mut result = text.to_string();
    // Replace longer spans first to avoid partial clobbering.
    let mut spans = parsed.spans;
    spans.sort_by_key(|span| std::cmp::Reverse(span.text.len()));

    for span in spans {
        let needle = span.text.trim();
        if needle.is_empty() || needle.starts_with(PLACEHOLDER_PREFIX) {
            continue;
        }
        if result.contains(needle) {
            let replacement = if let Some(v) = vault.as_deref_mut() {
                v.tokenize(&span.category, needle, format)
            } else {
                format!("[REDACTED_{}]", normalize_category(&span.category))
            };
            result = result.replace(needle, &replacement);
        }
    }

    Ok(result)
}

fn strip_markdown_json_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```json") {
        rest.trim_end_matches("```").trim()
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest.trim_end_matches("```").trim()
    } else {
        trimmed
    }
}
