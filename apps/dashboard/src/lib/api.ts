import type { User, Team, ApiKey, UsageData } from "./types";

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
  getKeys: (teamId: string) => fetchApi<ApiKey[]>(`/teams/${teamId}/keys`),
  createKey: (teamId: string, name: string) =>
    fetchApi<ApiKey>(`/teams/${teamId}/keys`, {
      method: "POST",
      body: JSON.stringify({ name }),
    }),
  revokeKey: (teamId: string, keyId: string) =>
    fetchApi<void>(`/teams/${teamId}/keys/${keyId}`, { method: "DELETE" }),

  getUsage: (teamId: string, period: string) =>
    fetchApi<UsageData[]>(`/teams/${teamId}/usage?period=${period}`),

  getConversations: () => fetchApi<any[]>("/conversations"),
  getConversation: (id: string) => fetchApi<any>(`/conversations/${id}`),
};
