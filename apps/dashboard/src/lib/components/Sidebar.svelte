<script lang="ts">
	import { page } from '$app/state';
	import { auth } from '#lib/stores/auth';
	import { theme } from '#lib/stores/theme';
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	const navItems = [
		{ path: '/dashboard', label: 'Overview', icon: 'dashboard' },
		{ path: '/dashboard/playground', label: 'Playground', icon: 'terminal' },
		{ path: '/dashboard/logs', label: 'Request Logs', icon: 'receipt_long' },
		{ path: '/dashboard/sessions', label: 'Agent Sessions', icon: 'smart_toy' },
		{ path: '/dashboard/teams', label: 'Teams', icon: 'group', admin: true },
		{ path: '/dashboard/users', label: 'Users', icon: 'manage_accounts', admin: true },
		{ path: '/dashboard/keys', label: 'Keys', icon: 'vpn_key' },
		{ path: '/dashboard/routing', label: 'Routing', icon: 'route', admin: true },
		{ path: '/dashboard/mcp', label: 'MCP Gateway', icon: 'hub', admin: true },
		{ path: '/dashboard/settings', label: 'Settings', icon: 'settings' },
	];

	let filteredItems = $derived(navItems.filter(item =>
		$auth.user?.role === 'admin' || !item.admin
	));

	function isActive(path: string): boolean {
		const currentPath = page.url.pathname;
		if (path === '/dashboard') {
			return currentPath === '/dashboard' || currentPath === '/dashboard/';
		}
		if (path === '/dashboard/teams') {
			return currentPath === path || currentPath.startsWith('/dashboard/teams/');
		}
		return currentPath === path || currentPath.startsWith(path + '/');
	}
</script>

<div class="flex h-screen">
	<aside class="w-[260px] flex flex-col shrink-0 relative border-r border-[var(--ghost-border)]" style="background: var(--surface-variant); backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px);">
		<div class="absolute inset-0 bg-[var(--surface-variant)]/60 backdrop-blur-xl -z-10"></div>

		<div class="p-6 pb-4">
			<div class="flex items-center gap-3">
				<div class="w-9 h-9 rounded-lg flex items-center justify-center shadow-xs" style="background: var(--brand-gradient)">
					<span class="material-symbols-outlined text-[var(--on-brand)]" style="font-size: 20px">bolt</span>
				</div>
				<div>
					<h1 class="text-base font-bold text-[var(--on-surface)]" style="font-family: 'Manrope', sans-serif">HyperInfer</h1>
					<p class="text-[10px] uppercase tracking-[0.15em] text-[var(--on-surface-variant)]">Precision AI</p>
				</div>
			</div>
		</div>

		<nav class="flex-1 px-3 space-y-1">
			{#each filteredItems as item}
				{@const active = isActive(item.path)}
				<a
					href={item.path}
				class="flex items-center gap-3 px-4 py-2.5 rounded-lg transition-all duration-200 group
					{active
						? 'text-[var(--primary)] bg-[var(--primary)]/10 font-semibold'
						: 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/70'}"
				>
					<span class="material-symbols-outlined transition-all duration-200 {active ? 'fill-1' : ''}" style="font-size: 20px">{item.icon}</span>
					<span class="text-sm font-medium">{item.label}</span>
				</a>
			{/each}
		</nav>

		<div class="p-3 space-y-1">
			<div class="flex items-center gap-3 px-4 py-3 rounded-lg bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)]">
				<div class="w-8 h-8 rounded-full flex items-center justify-center text-sm font-semibold text-[var(--on-brand)]" style="background: var(--brand-gradient)">
					{$auth.user?.email?.[0]?.toUpperCase() || '?'}
				</div>
				<div class="flex-1 min-w-0">
					<p class="text-sm font-medium text-[var(--on-surface)] truncate">{$auth.user?.email}</p>
					<p class="text-[10px] uppercase tracking-wider text-[var(--on-surface-variant)]">{$auth.user?.role || 'member'}</p>
				</div>
			</div>
			<div class="flex gap-1">
				<button
					class="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-xs font-medium text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/50 transition-all cursor-pointer"
					onclick={() => theme.toggle()}
				>
					<span class="material-symbols-outlined" style="font-size: 16px">{$theme === 'light' ? 'dark_mode' : 'light_mode'}</span>
					{$theme === 'light' ? 'Dark' : 'Light'}
				</button>
				<button
					class="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-xs font-medium text-[var(--error)] hover:bg-[var(--error)]/10 transition-all cursor-pointer"
					onclick={async () => { try { await auth.logout(); } finally { window.location.href = '/login'; } }}
				>
					<span class="material-symbols-outlined" style="font-size: 16px">logout</span>
					Logout
				</button>
			</div>
		</div>
	</aside>

	<main class="flex-1 overflow-auto">
		<div class="p-8 max-w-[1400px] mx-auto">
			{@render children()}
		</div>
	</main>
</div>
