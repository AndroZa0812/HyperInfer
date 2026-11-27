export interface User {
    id: string;
    email: string;
    role: 'admin' | 'member';
    team_id: string;
    created_at?: string;
}

export interface CreateUserRequest {
    team_id: string;
    email: string;
    role: 'admin' | 'member';
    password?: string;
}

export interface ModelAlias {
    id: string;
    team_id: string;
    alias: string;
    target_model: string;
    provider: string;
    created_at: string;
}

export interface CreateModelAliasRequest {
    team_id: string;
    alias: string;
    target_model: string;
    provider: string;
}

export interface Quota {
    id: string;
    team_id: string;
    rpm_limit: number;
    tpm_limit: number;
    updated_at: string;
}

export interface Team {
    id: string;
    name: string;
    budget_cents: number;
    created_at: string;
}

export interface ApiKey {
    id: string;
    name: string | null;
    prefix?: string;
    is_active: boolean;
    created_at: string;
    last_used_at?: string | null;
    expires_at?: string | null;
    budget_cents?: number | null;
    spent_cents?: number;
    allowed_models?: string[] | null;
    rpm_limit?: number | null;
    tpm_limit?: number | null;
}

export interface CreateApiKeyOptions {
    budget_cents?: number;
    allowed_models?: string[];
    rpm_limit?: number;
    tpm_limit?: number;
    expires_at?: string;
}

export interface CreateApiKeyResponse extends ApiKey {
    key: string;
}

export interface RoutingConfig {
    strategy: string;
    strategy_params?: Record<string, unknown>;
    fallback_config?: Record<string, unknown>;
    routing_groups?: Record<string, unknown>;
    updated_at?: string;
}

export interface RoutingHealth {
    active_deployments: number;
    strategy: string;
    status: string;
}

export interface UsageData {
    total_requests: number;
    total_input_tokens: number;
    total_output_tokens: number;
    avg_response_time_ms: number;
}

export interface Deployment {
    id: string;
    name: string;
    provider: string;
    model: string;
    api_key_ref?: string | null;
    base_url: string;
    is_active: boolean;
    weight: number;
    priority: number;
    max_tpm?: number;
    max_rpm?: number;
    cost_per_1k_input_tokens?: number;
    cost_per_1k_output_tokens?: number;
    metadata?: Record<string, unknown>;
    sort_order: number;
    created_at: string;
    updated_at: string;
}

export interface CreateDeploymentRequest {
    name: string;
    provider: string;
    model: string;
    api_key_ref?: string;
    base_url: string;
    is_active?: boolean;
    weight?: number;
    priority?: number;
    max_tpm?: number;
    max_rpm?: number;
    cost_per_1k_input_tokens?: number;
    cost_per_1k_output_tokens?: number;
    metadata?: Record<string, unknown>;
    sort_order?: number;
}

export interface DetectedProviderInfo {
    name: string;
    provider: string;
    models: string[];
}

export interface ServerConfig {
    standalone: boolean;
    mode: 'standalone' | 'standard';
    admin_email_hint?: string | null;
    detected_providers: DetectedProviderInfo[];
    endpoints: {
        openai_chat: string;
        openai_responses: string;
        anthropic_messages: string;
        gemini_generate: string;
        mcp_http: string;
        mcp_sse: string;
    };
}

export interface McpUpstream {
    name: string;
    transport: 'streamable_http' | 'sse';
    url: string;
    enabled: boolean;
    header_keys: string[];
}

export interface RegisterMcpUpstreamRequest {
    name: string;
    transport: 'streamable_http' | 'sse';
    url: string;
    headers?: Record<string, string>;
    enabled?: boolean;
}

export interface McpToolDefinition {
    name: string;
    description: string;
    inputSchema: Record<string, unknown>;
}

export interface McpToolsResponse {
    tools: McpToolDefinition[];
    catalog_meta_tools: McpToolDefinition[];
}

export interface McpTeamPolicy {
    allow_patterns: string[];
    deny_patterns: string[];
    catalog_mode: boolean;
}

export interface ModelPricing {
    input_per_1m_usd: number;
    output_per_1m_usd: number;
    cache_read_per_1m_usd?: number;
    cache_write_per_1m_usd?: number;
    reasoning_per_1m_usd?: number;
}

export interface ModelSpec {
    id: string;
    provider: string;
    name: string;
    context_window?: number;
    max_output_tokens?: number;
    pricing?: ModelPricing;
    reasoning: boolean;
    tool_call: boolean;
    source: string;
}

export interface CatalogResponse {
    source: string;
    updated_at_secs: number;
    count: number;
    models: ModelSpec[];
}

export interface OpenAiModelListItem {
    id: string;
    object: string;
    owned_by: string;
    context_window?: number;
    pricing?: ModelPricing;
}

export interface OpenAiModelListResponse {
    object: string;
    data: OpenAiModelListItem[];
}

export interface UsageLog {
    id: string;
    team_id: string;
    api_key_id: string;
    model: string;
    input_tokens: number;
    output_tokens: number;
    response_time_ms: number;
    recorded_at: string;
    status_code: number;
    cached_input_tokens: number;
    reasoning_tokens: number;
    cost_cents: number;
    cache_hit: boolean;
    failover_used: boolean;
    deployment_name?: string | null;
    stop_reason?: string | null;
}

export interface UsageLogsPage {
    logs: UsageLog[];
    total: number;
    limit: number;
    offset: number;
}

export interface UsageLogsFilter {
    team_id?: string;
    api_key_id?: string;
    model?: string;
    status_code?: number;
    cache_hit?: boolean;
    from?: string;
    to?: string;
    limit?: number;
    offset?: number;
}

export type PlaceholderFormat = 'indexed' | 'uuid';
export type SmartRedactionBackend = 'presidio' | 'llm';
export type SmartRedactionFailMode = 'fallback_regex' | 'fail_closed';

export interface SmartRedactionConfig {
    enabled: boolean;
    backend: SmartRedactionBackend;
    presidio_url: string;
    presidio_language: string;
    presidio_entities: string[];
    presidio_score_threshold: number;
    llm_base_url: string;
    llm_model: string;
    llm_api_key?: string;
    llm_categories: string[];
    timeout_ms: number;
    fail_mode: SmartRedactionFailMode;
}

export interface RedactionConfig {
    redact_api_keys: boolean;
    redact_bearer_tokens: boolean;
    redact_emails: boolean;
    redact_ssns: boolean;
    custom_patterns: [string, string][];
    redact_requests: boolean;
    redact_responses: boolean;
    reversible: boolean;
    placeholder_format: PlaceholderFormat;
    smart: SmartRedactionConfig;
}

export interface TestRedactionResponse {
    redacted: string;
    vault_entries?: Record<string, string>;
}

export interface SemanticCacheConfig {
    enabled: boolean;
    similarity_threshold: number;
    ttl_secs: number;
    embedding_provider: string;
    embedding_model: string;
    sse_replay: boolean;
}

export interface GatewaySettings {
    pipeline_plugins: string[];
    redaction: RedactionConfig;
    cache: SemanticCacheConfig;
}
