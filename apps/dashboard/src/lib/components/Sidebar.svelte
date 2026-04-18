<script lang="ts">
	import { page } from '$app/stores';
	import { auth } from '$lib/stores/auth';
	import { theme } from '$lib/stores/theme';
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	const navItems = [
        { path: '/dashboard/teams', label: 'Teams', icon: 'users', admin: true },
        { path: '/dashboard/keys', label: 'Keys', icon: 'key' },
        { path: '/dashboard/conversations', label: 'Conversations', icon: 'chat' },
        { path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
    ];

    let filteredItems = $derived(navItems.filter(item =>
        $auth.user?.role === 'admin' || !item.admin
    ));

    function isActive(path: string): boolean {
        const currentPath = $page.url.pathname;
        if (path === '/dashboard/teams') {
            return currentPath === path || currentPath.startsWith('/dashboard/teams/');
        }
        return currentPath === path || currentPath.startsWith(path + '/');
    }
</script>

<div class="flex h-screen">
    <aside class="w-64 bg-[var(--bg-primary)] border-r border-[var(--bg-secondary)] flex flex-col shrink-0">
        <div class="p-4 border-b border-[var(--bg-secondary)]">
            <h1 class="text-xl font-bold text-[var(--accent)]">HyperInfer</h1>
        </div>

        <nav class="flex-1 p-4 space-y-2">
            {#each filteredItems as item}
                <a
                    href={item.path}
                    class="flex items-center gap-3 px-4 py-2 rounded-lg transition-colors
                           {isActive(item.path)
                               ? 'bg-[var(--accent)] text-white'
                               : 'hover:bg-[var(--bg-secondary)]'}"
                >
                    {#if item.icon === 'users'}
                        <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4.354a4 4 0 110 5.292M15 21H3v-1a6 6 0 0112 0v1zm0 0h6v-1a6 6 0 00-9-5.197M13 7a4 4 0 11-8 0 4 4 0 018 0z" /></svg>
                    {:else if item.icon === 'key'}
                        <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17h9v-2l-3.743-3.743A6 6 0 0113.257 2.257L9 6h9v2z" /></svg>
                    {:else if item.icon === 'chat'}
                        <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" /></svg>
                    {:else if item.icon === 'settings'}
                        <svg class="w-5 h-5" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" /><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" /></svg>
                    {/if}
                    <span>{item.label}</span>
                </a>
            {/each}
        </nav>

        <div class="p-4 border-t border-[var(--bg-secondary)]">
            <div class="flex items-center gap-3 mb-3">
                <div class="w-8 h-8 rounded-full bg-[var(--accent)] flex items-center justify-center text-white">
                    {$auth.user?.email?.[0]?.toUpperCase() || '?'}
                </div>
                <span class="text-sm truncate">{$auth.user?.email}</span>
            </div>
            <div class="flex gap-2">
                <button
                    class="text-sm text-[var(--accent)]"
                    onclick={() => theme.toggle()}
                >
                    {$theme === 'light' ? 'Dark' : 'Light'}
                </button>
                <button
                    class="text-sm text-red-500"
                    onclick={async () => { try { await auth.logout(); } finally { window.location.href = '/login'; } }}
                >
                    Logout
                </button>
            </div>
        </div>
    </aside>

    <main class="flex-1 overflow-auto p-6">
        {@render children()}
    </main>
</div>