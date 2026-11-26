# PII & Secret Redaction Guide

HyperInfer's built-in **Redaction Plugin** intercepts prompts and responses in the gateway plugin pipeline to scrub API keys, bearer tokens, emails, SSNs, custom regex patterns, and semantic PII before requests leave your network.

It supports three composable layers:

1. **Regex Redaction** — ultra-fast deterministic scrubbing for API keys, bearer tokens, emails, SSNs, and custom regular expressions.
2. **Reversible Redaction (Opt-In)** — replaces detected values with deterministic per-request placeholders (indexed or UUID) and transparently restores the original values in the upstream LLM's response (including streamed SSE chunks).
3. **Smart Redaction (Opt-In)** — delegates semantic PII and secret detection to either a **Microsoft Presidio Analyzer** sidecar or a **local / trusted LLM** before the request is sent to the upstream model.

---

## 1. Regex Redaction (Baseline)

By default, `RedactionPlugin` runs in **one-way regex mode** (`reversible: false`, `smart.enabled: false`), replacing matches with static placeholders such as `[REDACTED_API_KEY]`, `[REDACTED_EMAIL]`, and `[REDACTED_SSN]`.

### Built-in Rules

| Field | Default | Matches |
| :--- | :--- | :--- |
| `redact_api_keys` | `true` | Common provider API key formats (`sk-...`, `sk-ant-...`, `AIza...`, `ghp_...`, `xoxb-...`, etc.) |
| `redact_bearer_tokens` | `true` | `Bearer <token>` authorization strings (preserving the `Bearer ` prefix) |
| `redact_emails` | `true` | Email addresses |
| `redact_ssns` | `true` | US Social Security Numbers (`XXX-XX-XXXX`) |
| `custom_patterns` | `[]` | Array of `[regex, replacement]` tuples applied in order |
| `redact_requests` | `true` | Apply redaction to outbound chat messages and tool-call arguments |
| `redact_responses` | `true` | Apply one-way redaction to inbound responses (ignored when `reversible: true`) |

---

## 2. Reversible Redaction (Opt-In)

When `reversible` is set to `true`, HyperInfer creates a **per-request `RedactionVault`** in `PluginContext.metadata`:

1. **Request (`pre_request`)**: Each matched secret or PII value is tokenized into a unique placeholder. If the same secret appears multiple times in the prompt, it receives the **same** placeholder so the upstream LLM preserves referential integrity.
2. **Upstream LLM**: The model only ever sees placeholders (for example, `[REDACTED_API_KEY_1]` or `[REDACTED_EMAIL_a1b2c3d4]`).
3. **Response (`post_response` & SSE streaming)**: When the upstream LLM echoes a placeholder back, HyperInfer swaps it with the original value from the per-request vault before returning the response to the caller, then drops the vault.

!!! warning "Telemetry, Logs & Cache Safety"
    Original unredacted values are **never** written to logs, traces, or the semantic cache. The `RedactionVault` lives strictly in memory for the lifetime of a single request and is removed as soon as response re-hydration completes.

### Placeholder Formats

| `placeholder_format` | Example | Best For |
| :--- | :--- | :--- |
| `"indexed"` *(default)* | `[REDACTED_API_KEY_1]`, `[REDACTED_EMAIL_2]` | Compact, readable tokens that LLMs echo reliably |
| `"uuid"` | `[REDACTED_API_KEY_a1b2c3d4]` | Non-sequential opaque tokens |

### Enabling Reversible Redaction via Admin API

```bash
curl -X PUT http://localhost:8080/v1/admin/gateway/redaction \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "redact_api_keys": true,
    "redact_bearer_tokens": true,
    "redact_emails": true,
    "redact_ssns": true,
    "custom_patterns": [],
    "redact_requests": true,
    "redact_responses": false,
    "reversible": true,
    "placeholder_format": "indexed"
  }'
```

### Streaming (SSE) Re-hydration

For streaming chat completions, HyperInfer provides `StreamRehydrator` in `hyperinfer-core` to handle placeholders that are split across adjacent SSE token deltas (for example, `"[REDACTED_"` in chunk 1 and `"EMAIL_1]"` in chunk 2):

```rust
use hyperinfer_core::{RedactionVault, StreamRehydrator};

let mut rehydrator = StreamRehydrator::new(vault);
for delta in sse_chunks {
    let restored = rehydrator.push_chunk(&delta);
    if !restored.is_empty() {
        emit(restored);
    }
}
let tail = rehydrator.flush();
if !tail.is_empty() {
    emit(tail);
}
```

---

## 3. Smart Redaction — Microsoft Presidio Connector

[Microsoft Presidio](https://microsoft.github.io/presidio/) is an open-source PII detection engine backed by spaCy NER models and pattern recognizers. HyperInfer integrates directly with the `presidio-analyzer` HTTP API (`POST /analyze`) so names, locations, phone numbers, credit cards, IBANs, and other context-dependent PII are detected before prompts reach external LLM providers.

### Step 1: Run Microsoft Presidio Analyzer

=== "Docker"

    ```bash
    docker run -d \
      --name presidio-analyzer \
      -p 5001:3000 \
      mcr.microsoft.com/presidio-analyzer:latest
    ```

    Verify the sidecar is healthy:

    ```bash
    curl http://127.0.0.1:5001/health
    ```

=== "Docker Compose"

    Add `presidio-analyzer` alongside `hyperinfer-server` on the same Docker network:

    ```yaml
    services:
      presidio-analyzer:
        image: mcr.microsoft.com/presidio-analyzer:latest
        restart: unless-stopped
        ports:
          - "5001:3000"

      hyperinfer-server:
        image: ghcr.io/androza0812/hyperinfer:latest
        depends_on:
          - presidio-analyzer
        # Point smart.presidio_url to http://presidio-analyzer:3000
    ```

!!! tip "Container-to-Container URL"
    When `hyperinfer-server` and `presidio-analyzer` run in the same Docker Compose network or Kubernetes pod, set `presidio_url` to `http://presidio-analyzer:3000` (the container's internal port is `3000`). When running `hyperinfer-server` on the host with `-p 5001:3000`, use `http://127.0.0.1:5001`.

### Step 2: Configure the Presidio Connector

Configure the connector either in the **Dashboard UI** (**Settings → Plugins & Cache → PII & Secret Redaction → Smart Redaction**) or via the Admin API:

```bash
curl -X PUT http://localhost:8080/v1/admin/gateway/redaction \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "redact_api_keys": true,
    "redact_bearer_tokens": true,
    "redact_emails": true,
    "redact_ssns": true,
    "custom_patterns": [],
    "redact_requests": true,
    "redact_responses": false,
    "reversible": true,
    "placeholder_format": "indexed",
    "smart": {
      "enabled": true,
      "backend": "presidio",
      "presidio_url": "http://127.0.0.1:5001",
      "presidio_language": "en",
      "presidio_entities": [
        "PERSON",
        "LOCATION",
        "PHONE_NUMBER",
        "CREDIT_CARD",
        "IBAN_CODE",
        "US_SSN",
        "EMAIL_ADDRESS"
      ],
      "presidio_score_threshold": 0.5,
      "timeout_ms": 3000,
      "fail_mode": "fallback_regex"
    }
  }'
```

### Step 3: Verify with Live Preview

Test the live connector using the Admin API (or the **Live Redaction Preview** box in the Dashboard):

```bash
curl -X POST http://localhost:8080/v1/admin/gateway/redaction/test \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"sample": "Contact Alice Johnson in Berlin at +1-415-555-0199"}'
```

Example response with `reversible: true`:

```json
{
  "redacted": "Contact [REDACTED_PERSON_1] in [REDACTED_LOCATION_2] at [REDACTED_PHONE_NUMBER_3]",
  "vault_entries": {
    "[REDACTED_PERSON_1]": "Alice Johnson",
    "[REDACTED_LOCATION_2]": "Berlin",
    "[REDACTED_PHONE_NUMBER_3]": "+1-415-555-0199"
  }
}
```

### Presidio Parameters

| Field | Default | Description |
| :--- | :--- | :--- |
| `presidio_url` | `"http://127.0.0.1:5001"` | Base URL of the `presidio-analyzer` service (`http://` or `https://`). HyperInfer calls `POST <presidio_url>/analyze`. |
| `presidio_language` | `"en"` | ISO language code passed to Presidio (`en`, `de`, `es`, etc. depending on installed spaCy models). |
| `presidio_entities` | `["PERSON", "LOCATION", "PHONE_NUMBER", "CREDIT_CARD", "IBAN_CODE", "US_SSN", "EMAIL_ADDRESS"]` | List of Presidio entity types to detect. Leave empty to run all registered Presidio recognizers. |
| `presidio_score_threshold` | `0.5` | Minimum confidence score (`0.0`–`1.0`) required to redact a detected span. |

---

## 4. Smart Redaction — Local Small LLM or Trusted Provider

If you want to redact domain-specific secrets (internal codenames, customer IDs, proprietary financial figures) that NER models miss, you can configure a **local small LLM** (such as Ollama, vLLM, or llama.cpp) or a **trusted internal OpenAI-compatible endpoint** as a pre-pass redactor.

### Example: Local Ollama (`qwen2.5:3b`)

1. Pull a fast local model:

    ```bash
    ollama pull qwen2.5:3b
    ```

2. Configure Smart Redaction with `backend: "llm"`:

    ```bash
    curl -X PUT http://localhost:8080/v1/admin/gateway/redaction \
      -H "Authorization: Bearer $ADMIN_TOKEN" \
      -H "Content-Type: application/json" \
      -d '{
        "redact_api_keys": true,
        "redact_bearer_tokens": true,
        "redact_emails": true,
        "redact_ssns": true,
        "custom_patterns": [],
        "redact_requests": true,
        "redact_responses": false,
        "reversible": true,
        "placeholder_format": "indexed",
        "smart": {
          "enabled": true,
          "backend": "llm",
          "llm_base_url": "http://127.0.0.1:11434/v1",
          "llm_model": "qwen2.5:3b",
          "llm_categories": ["pii", "secrets", "financial", "internal_names"],
          "timeout_ms": 5000,
          "fail_mode": "fallback_regex"
        }
      }'
    ```

### LLM Parameters

| Field | Default | Description |
| :--- | :--- | :--- |
| `llm_base_url` | `"http://127.0.0.1:11434/v1"` | OpenAI-compatible base URL (`POST <llm_base_url>/chat/completions`). |
| `llm_model` | `"qwen2.5:3b"` | Model identifier to invoke for span extraction. |
| `llm_api_key` | `null` | Optional bearer token sent as `Authorization: Bearer <key>` when calling a trusted remote provider. |
| `llm_categories` | `["pii", "secrets", "financial", "internal_names"]` | Sensitivity categories passed to the redactor prompt. |

!!! info "Recursion Guard"
    Point `llm_base_url` directly at your local Ollama/vLLM instance or trusted upstream provider rather than looping back through the public gateway proxy. Outbound LLM redaction calls include the `X-HyperInfer-Skip-Redaction: 1` marker header for internal interceptors, and programmatic callers can set `SKIP_REDACTION_METADATA_KEY` in `PluginContext.metadata` to bypass redaction for trusted internal sub-requests.

---

## 5. Failure Modes & Timeouts

Smart redaction calls are bounded by `smart.timeout_ms` (default `3000` ms). If the Presidio sidecar or local LLM is unreachable, times out, or returns an invalid payload, HyperInfer follows `smart.fail_mode`:

| `fail_mode` | Behavior on Smart Redactor Failure |
| :--- | :--- |
| `"fallback_regex"` *(default)* | Logs a warning and continues with the deterministic regex-redacted text so traffic is not disrupted. |
| `"fail_closed"` | Short-circuits the request with HTTP **`503 Service Unavailable`** so potentially unredacted PII is never forwarded to an untrusted upstream LLM. |

!!! tip "Composing All Three Layers"
    Regex redaction always runs **before** Smart Redaction, so high-confidence secrets (like API keys and bearer tokens) are scrubbed locally before text even reaches Presidio or your local LLM. Enabling `reversible: true` shares a single `RedactionVault` across both the regex pass and the smart pass, restoring all tokens seamlessly in the response.
