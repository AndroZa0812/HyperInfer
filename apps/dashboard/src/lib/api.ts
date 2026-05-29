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
  revokeKey: (id: string) =>
    fetchApi<ApiKey>(`/api_keys/${id}/revoke`, { method: "POST" }),
  getKeys: async (teamId: string) => {
    console.warn('getKeys not implemented - returning empty array');
    return [];
  },
  createKey: async (teamId: string, name: string) => {
    console.warn('createKey not implemented');
    throw new Error('Not implemented');
  },

  getUsage: async (teamId: string, period: string) => {
    console.warn('getUsage not implemented - returning empty array');
    return [];
  },

  getConversations: async () => {
    console.warn('getConversations not implemented - returning empty array');
    return [];
  },
  getConversation: async (id: string) => {
    console.warn('getConversation not implemented');
    throw new Error('Not implemented');
  },
};
