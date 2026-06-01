export interface User {
    id: string;
    email: string;
    role: 'admin' | 'member';
    team_id: string;
}

export interface Team {
    id: string;
    name: string;
    budget_cents: number;
    created_at: string;
}

export interface ApiKey {
    id: string;
    name: string;
    prefix: string;
    is_active: boolean;
    created_at: string;
    last_used_at?: string;
}

export interface UsageData {
    date: string;
    tokens: number;
    cost: number;
    latency_ms: number;
}

export interface Deployment {
    id: string;
    name: string;
    provider: string;
    model: string;
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
