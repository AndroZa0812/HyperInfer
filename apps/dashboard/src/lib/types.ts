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
