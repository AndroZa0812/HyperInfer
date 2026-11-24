import type { User, Team, ApiKey, UsageData, Deployment, CreateDeploymentRequest } from "./types";

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

  getKey: (id: string) => fetchApi<ApiKey>(`/api_keys/${id}`),
  revokeKey: (id: string) =>
    fetchApi<ApiKey>(`/api_keys/${id}/revoke`, { method: "POST" }),
  getKeys: (teamId?: string) => {
    const params = teamId ? `?team_id=${encodeURIComponent(teamId)}` : '';
    return fetchApi<ApiKey[]>(`/api_keys${params}`);
  },
  createKey: (teamId: string, name: string) =>
    fetchApi<ApiKey>("/api_keys", {
      method: "POST",
      body: JSON.stringify({ team_id: teamId, name }),
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
};
