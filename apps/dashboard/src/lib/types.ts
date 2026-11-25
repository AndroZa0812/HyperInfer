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
