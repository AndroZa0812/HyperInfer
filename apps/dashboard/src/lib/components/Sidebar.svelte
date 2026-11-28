<script lang="ts">
	import { page } from '$app/state';
	import { auth } from '#lib/stores/auth';
	import { theme } from '#lib/stores/theme';
	import { onMount, type Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	const STORAGE_KEY = 'hyperinfer-sidebar-collapsed';

	let collapsed = $state(false);
	let userToggled = $state(false);
	let isMac = $state(false);
	let shortcutHint = $derived(isMac ? '⌘B' : 'Ctrl+B');

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

	let filteredItems = $derived(
		navItems.filter((item) => $auth.user?.role === 'admin' || !item.admin)
	);

	onMount(() => {
		const platform = (navigator.platform || navigator.userAgent || '').toLowerCase();
		isMac = platform.includes('mac');

		try {
			const saved = localStorage.getItem(STORAGE_KEY);
			if (saved === 'true' || saved === 'false') {
				collapsed = saved === 'true';
				userToggled = true;
			} else {
				collapsed = window.innerWidth < 900;
			}
		} catch {
			collapsed = window.innerWidth < 900;
		}

		const onResize = () => {
			if (!userToggled) {
				collapsed = window.innerWidth < 900;
			}
		};
		window.addEventListener('resize', onResize, { passive: true });
		return () => window.removeEventListener('resize', onResize);
	});

	function toggleSidebar() {
		collapsed = !collapsed;
		userToggled = true;
		try {
			localStorage.setItem(STORAGE_KEY, String(collapsed));
		} catch {
			// Ignore storage errors
		}
	}

	function handleWindowKeydown(e: KeyboardEvent) {
		if (e.altKey || e.shiftKey) return;
		const modPressed = isMac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey;
		if (modPressed && e.key.toLowerCase() === 'b') {
			e.preventDefault();
			toggleSidebar();
		}
	}

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

<svelte:window onkeydown={handleWindowKeydown} />

<div class="flex h-screen overflow-hidden">
	<aside
		class="{collapsed ? 'w-[64px]' : 'w-[256px]'} flex flex-col shrink-0 relative border-r border-[var(--ghost-border)] transition-[width] duration-200 ease-out select-none"
		style="background: var(--surface-variant); backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px);"
	>
		<div class="absolute inset-0 bg-[var(--surface-variant)]/60 backdrop-blur-xl -z-10"></div>

		{#if collapsed}
			<div class="px-2 pt-4 pb-3 flex flex-col items-center gap-2 border-b border-[var(--ghost-border)]">
				<div
					class="w-9 h-9 rounded-lg flex items-center justify-center shadow-xs shrink-0"
					style="background: var(--brand-gradient)"
					title="HyperInfer — Precision AI"
				>
					<span class="material-symbols-outlined text-[var(--on-brand)]" style="font-size: 20px">bolt</span>
				</div>
				<button
					type="button"
					aria-label="Expand sidebar"
					aria-expanded="false"
					title="Expand sidebar ({shortcutHint})"
					onclick={toggleSidebar}
					class="w-9 h-9 rounded-lg flex items-center justify-center text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/80 transition-colors cursor-pointer"
				>
					<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
						<rect x="3" y="3" width="18" height="18" rx="3" />
						<line x1="9" y1="3" x2="9" y2="21" />
						<path d="M13 9l3 3-3 3" />
					</svg>
				</button>
			</div>
		{:else}
			<div class="px-4 pt-5 pb-4 flex items-center justify-between gap-2">
				<div class="flex items-center gap-3 min-w-0">
					<div
						class="w-9 h-9 rounded-lg flex items-center justify-center shadow-xs shrink-0"
						style="background: var(--brand-gradient)"
					>
						<span class="material-symbols-outlined text-[var(--on-brand)]" style="font-size: 20px">bolt</span>
					</div>
					<div class="min-w-0">
						<h1 class="text-base font-bold text-[var(--on-surface)] truncate" style="font-family: 'Manrope', sans-serif">
							HyperInfer
						</h1>
						<p class="text-[10px] uppercase tracking-[0.15em] text-[var(--on-surface-variant)] truncate">
							Precision AI
						</p>
					</div>
				</div>
				<button
					type="button"
					aria-label="Collapse sidebar"
					aria-expanded="true"
					title="Collapse sidebar ({shortcutHint})"
					onclick={toggleSidebar}
					class="w-8 h-8 rounded-lg flex items-center justify-center shrink-0 text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/80 transition-colors cursor-pointer"
				>
					<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
						<rect x="3" y="3" width="18" height="18" rx="3" />
						<line x1="9" y1="3" x2="9" y2="21" />
						<path d="M16 9l-3 3 3 3" />
					</svg>
				</button>
			</div>
		{/if}

		<nav class="flex-1 {collapsed ? 'px-2 py-2' : 'px-3 py-1'} space-y-1 overflow-y-auto">
			{#each filteredItems as item}
				{@const active = isActive(item.path)}
				<a
					href={item.path}
					aria-label={item.label}
					title={collapsed ? item.label : undefined}
					class="flex items-center {collapsed ? 'justify-center w-10 h-10 mx-auto' : 'gap-3 px-3.5 py-2.5'} rounded-lg transition-all duration-200 group
					{active
						? 'text-[var(--primary)] bg-[var(--primary)]/10 font-semibold'
						: 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/70'}"
				>
					<span
						aria-hidden="true"
						class="material-symbols-outlined shrink-0 transition-all duration-200 {active ? 'fill-1' : ''}"
						style="font-size: 20px"
					>
						{item.icon}
					</span>
					{#if !collapsed}
						<span class="text-sm font-medium truncate">{item.label}</span>
					{/if}
				</a>
			{/each}
		</nav>

		{#if collapsed}
			<div class="p-2 border-t border-[var(--ghost-border)] flex flex-col items-center gap-1.5">
				<div
					class="w-8 h-8 rounded-full flex items-center justify-center text-xs font-semibold text-[var(--on-brand)] shrink-0"
					style="background: var(--brand-gradient)"
					title="{$auth.user?.email || ''} ({$auth.user?.role || 'member'})"
				>
					{$auth.user?.email?.[0]?.toUpperCase() || '?'}
				</div>
				<button
					type="button"
					aria-label="Toggle theme"
					title="Switch to {$theme === 'light' ? 'Dark' : 'Light'} mode"
					class="w-9 h-9 flex items-center justify-center rounded-lg text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/60 transition-all cursor-pointer"
					onclick={() => theme.toggle()}
				>
					<span aria-hidden="true" class="material-symbols-outlined" style="font-size: 18px">
						{$theme === 'light' ? 'dark_mode' : 'light_mode'}
					</span>
				</button>
				<button
					type="button"
					aria-label="Logout"
					title="Logout"
					class="w-9 h-9 flex items-center justify-center rounded-lg text-[var(--error)] hover:bg-[var(--error)]/10 transition-all cursor-pointer"
					onclick={async () => {
						try {
							await auth.logout();
						} finally {
							window.location.href = '/login';
						}
					}}
				>
					<span aria-hidden="true" class="material-symbols-outlined" style="font-size: 18px">logout</span>
				</button>
			</div>
		{:else}
			<div class="p-3 space-y-1">
				<div class="flex items-center gap-3 px-4 py-3 rounded-lg bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)]">
					<div
						class="w-8 h-8 rounded-full flex items-center justify-center text-sm font-semibold text-[var(--on-brand)] shrink-0"
						style="background: var(--brand-gradient)"
					>
						{$auth.user?.email?.[0]?.toUpperCase() || '?'}
					</div>
					<div class="flex-1 min-w-0">
						<p class="text-sm font-medium text-[var(--on-surface)] truncate">{$auth.user?.email}</p>
						<p class="text-[10px] uppercase tracking-wider text-[var(--on-surface-variant)]">
							{$auth.user?.role || 'member'}
						</p>
					</div>
				</div>
				<div class="flex gap-1">
					<button
						type="button"
						class="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-xs font-medium text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/50 transition-all cursor-pointer"
						onclick={() => theme.toggle()}
					>
						<span aria-hidden="true" class="material-symbols-outlined" style="font-size: 16px">
							{$theme === 'light' ? 'dark_mode' : 'light_mode'}
						</span>
						{$theme === 'light' ? 'Dark' : 'Light'}
					</button>
					<button
						type="button"
						class="flex-1 flex items-center justify-center gap-2 px-3 py-2 rounded-lg text-xs font-medium text-[var(--error)] hover:bg-[var(--error)]/10 transition-all cursor-pointer"
						onclick={async () => {
							try {
								await auth.logout();
							} finally {
								window.location.href = '/login';
							}
						}}
					>
						<span aria-hidden="true" class="material-symbols-outlined" style="font-size: 16px">logout</span>
						Logout
					</button>
				</div>
			</div>
		{/if}
	</aside>

	<main class="flex-1 min-w-0 overflow-auto">
		<div class="p-4 sm:p-6 lg:p-8 max-w-[1400px] mx-auto">
			{@render children()}
		</div>
	</main>
</div>
