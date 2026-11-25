import type {
  User,
  CreateUserRequest,
  Team,
  ApiKey,
  CreateApiKeyOptions,
  CreateApiKeyResponse,
  UsageData,
  Deployment,
  CreateDeploymentRequest,
  ModelAlias,
  CreateModelAliasRequest,
  Quota,
  RoutingConfig,
  RoutingHealth,
  ServerConfig,
  McpUpstream,
  RegisterMcpUpstreamRequest,
  McpToolsResponse,
  McpTeamPolicy,
} from "./types";

const API_BASE = "/v1";

async function fetchApi<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    ...options,
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      ...options?.headers,
    },
  });

  if (!response.ok) {
    throw new Error(`API error: ${response.status}`);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return response.json();
}

export const api = {
  login: (email: string, password: string) =>
    fetchApi<User>("/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),

  logout: () => fetchApi<void>("/auth/logout", { method: "POST" }),

  changePassword: (currentPassword: string, newPassword: string) =>
    fetchApi<void>("/auth/change-password", {
      method: "POST",
      body: JSON.stringify({
        current_password: currentPassword,
        new_password: newPassword,
      }),
    }),

  me: () => fetchApi<User>("/auth/me"),

  getTeams: () => fetchApi<Team[]>("/teams"),
  getTeam: (id: string) => fetchApi<Team>(`/teams/${id}`),
  createTeam: (name: string, budget_cents: number) =>
    fetchApi<Team>("/teams", {
      method: "POST",
      body: JSON.stringify({ name, budget_cents }),
    }),
  updateTeam: (id: string, name: string, budget_cents: number) =>
    fetchApi<Team>(`/teams/${id}`, {
      method: "PUT",
      body: JSON.stringify({ name, budget_cents }),
    }),
  deleteTeam: (id: string) =>
    fetchApi<void>(`/teams/${id}`, { method: "DELETE" }),

  getUsers: (teamId?: string) => {
    const params = teamId ? `?team_id=${encodeURIComponent(teamId)}` : '';
    return fetchApi<User[]>(`/users${params}`);
  },
  createUser: (req: CreateUserRequest) =>
    fetchApi<User>("/users", {
      method: "POST",
      body: JSON.stringify(req),
    }),
  updateUserRole: (id: string, role: 'admin' | 'member') =>
    fetchApi<User>(`/users/${id}/role`, {
      method: "PUT",
      body: JSON.stringify({ role }),
    }),
  deleteUser: (id: string) =>
    fetchApi<void>(`/users/${id}`, { method: "DELETE" }),

  getModelAliases: (teamId: string) =>
    fetchApi<ModelAlias[]>(`/model_aliases?team_id=${encodeURIComponent(teamId)}`),
  createModelAlias: (req: CreateModelAliasRequest) =>
    fetchApi<ModelAlias>("/model_aliases", {
      method: "POST",
      body: JSON.stringify(req),
    }),
  deleteModelAlias: (id: string) =>
    fetchApi<void>(`/model_aliases/${id}`, { method: "DELETE" }),

  getQuota: (teamId: string) => fetchApi<Quota>(`/quotas/${teamId}`),
  upsertQuota: (teamId: string, rpm_limit: number, tpm_limit: number) =>
    fetchApi<Quota>(`/quotas/${teamId}`, {
      method: "PUT",
      body: JSON.stringify({ rpm_limit, tpm_limit }),
    }),

  getKey: (id: string) => fetchApi<ApiKey>(`/api_keys/${id}`),
  revokeKey: (id: string) =>
    fetchApi<ApiKey>(`/api_keys/${id}/revoke`, { method: "POST" }),
  getKeys: (teamId?: string) => {
    const params = teamId ? `?team_id=${encodeURIComponent(teamId)}` : '';
    return fetchApi<ApiKey[]>(`/api_keys${params}`);
  },
  createKey: (teamId: string, name: string, opts?: CreateApiKeyOptions) =>
    fetchApi<CreateApiKeyResponse>("/api_keys", {
      method: "POST",
      body: JSON.stringify({ team_id: teamId, name, ...opts }),
    }),

  getUsage: (teamId?: string) => {
    const params = teamId ? `?team_id=${encodeURIComponent(teamId)}` : '';
    return fetchApi<UsageData>(`/usage/summary${params}`);
  },

  // Deployment management
  getDeployments: (model: string, isActive?: boolean) => {
    const params = new URLSearchParams({ model });
    if (isActive !== undefined) {
      params.set('is_active', isActive.toString());
    }
    return fetchApi<Deployment[]>(`/deployments?${params.toString()}`);
  },
  getDeployment: (id: string) => fetchApi<Deployment>(`/deployments/${id}`),
  createDeployment: (req: CreateDeploymentRequest) =>
    fetchApi<Deployment>("/deployments", {
      method: "POST",
      body: JSON.stringify(req),
    }),
  updateDeployment: (id: string, req: CreateDeploymentRequest) =>
    fetchApi<Deployment>(`/deployments/${id}`, {
      method: "PUT",
      body: JSON.stringify(req),
    }),
  deleteDeployment: (id: string) =>
    fetchApi<void>(`/deployments/${id}`, { method: "DELETE" }),

  getRoutingConfig: () => fetchApi<RoutingConfig>("/admin/routing/config"),
  updateRoutingConfig: (strategy: string) =>
    fetchApi<RoutingConfig>("/admin/routing/config", {
      method: "PUT",
      body: JSON.stringify({ strategy }),
    }),
  getRoutingHealth: () => fetchApi<RoutingHealth>("/admin/routing/health"),

  // Server runtime config / standalone detection (HYP-74)
  getServerConfig: () => fetchApi<ServerConfig>("/auth/config"),

  // MCP Gateway management (HYP-70)
  getMcpUpstreams: () => fetchApi<McpUpstream[]>("/admin/mcp/upstreams"),
  registerMcpUpstream: (req: RegisterMcpUpstreamRequest) =>
    fetchApi<McpUpstream>("/admin/mcp/upstreams", {
      method: "POST",
      body: JSON.stringify(req),
    }),
  removeMcpUpstream: (name: string) =>
    fetchApi<void>(`/admin/mcp/upstreams/${encodeURIComponent(name)}`, {
      method: "DELETE",
    }),
  getMcpTools: () => fetchApi<McpToolsResponse>("/admin/mcp/tools"),
  getMcpTeamPolicy: (teamId: string) =>
    fetchApi<McpTeamPolicy>(`/admin/mcp/policies/${encodeURIComponent(teamId)}`),
  updateMcpTeamPolicy: (teamId: string, policy: McpTeamPolicy) =>
    fetchApi<McpTeamPolicy>(`/admin/mcp/policies/${encodeURIComponent(teamId)}`, {
      method: "PUT",
      body: JSON.stringify(policy),
    }),
};
