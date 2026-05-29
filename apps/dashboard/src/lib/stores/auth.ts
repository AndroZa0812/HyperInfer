import { writable } from 'svelte/store';
import { api } from '$lib/api';
import type { User } from '$lib/types';

function createAuthStore() {
    const { subscribe, set } = writable<{ user: User | null; loading: boolean }>({
        user: null,
        loading: true,
    });

    return {
        subscribe,
        init: async () => {
            try {
                const user = await api.me();
                set({ user, loading: false });
            } catch {
                set({ user: null, loading: false });
            }
        },
        login: async (email: string, password: string) => {
            const user = await api.login(email, password);
            set({ user, loading: false });
        },
        logout: async () => {
            await api.logout();
            set({ user: null, loading: false });
        },
    };
}

export const auth = createAuthStore();
