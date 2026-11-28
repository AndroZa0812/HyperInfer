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
		class="{collapsed ? 'w-[84px]' : 'w-[256px]'} flex flex-col shrink-0 relative border-r border-[var(--ghost-border)] transition-[width] duration-200 ease-out select-none overflow-hidden"
		style="background: var(--surface-variant); backdrop-filter: blur(20px); -webkit-backdrop-filter: blur(20px);"
	>
		<div class="absolute inset-0 bg-[var(--surface-variant)]/60 backdrop-blur-xl -z-10"></div>

		<!-- Single-line fixed-height header: lightning icon + disappearing title + expand/collapse square -->
		<div class="h-16 px-2.5 flex items-center justify-between gap-1 shrink-0 border-b border-[var(--ghost-border)]">
			<div class="flex items-center min-w-0">
				<div
					class="w-8 h-8 rounded-lg flex items-center justify-center shadow-xs shrink-0"
					style="background: var(--brand-gradient)"
					title="HyperInfer — Precision AI"
				>
					<span aria-hidden="true" class="material-symbols-outlined text-[var(--on-brand)]" style="font-size: 19px">bolt</span>
				</div>
				<div
					class="overflow-hidden whitespace-nowrap transition-all duration-200 ease-out {collapsed
						? 'max-w-0 opacity-0 ml-0'
						: 'max-w-[148px] opacity-100 ml-2.5'}"
				>
					<h1 class="text-sm font-bold text-[var(--on-surface)] leading-tight" style="font-family: 'Manrope', sans-serif">
						HyperInfer
					</h1>
					<p class="text-[10px] uppercase tracking-[0.14em] text-[var(--on-surface-variant)] leading-tight mt-0.5">
						Precision AI
					</p>
				</div>
			</div>

			<button
				type="button"
				aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'}
				aria-expanded={!collapsed}
				title="{collapsed ? 'Expand' : 'Collapse'} sidebar ({shortcutHint})"
				onclick={toggleSidebar}
				class="w-7 h-7 rounded-md flex items-center justify-center shrink-0 text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/80 transition-colors cursor-pointer"
			>
				<svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
					<rect x="3" y="3" width="18" height="18" rx="3" />
					<line x1="9" y1="3" x2="9" y2="21" />
					{#if collapsed}
						<path d="M13 9l3 3-3 3" />
					{:else}
						<path d="M16 9l-3 3 3 3" />
					{/if}
				</svg>
			</button>
		</div>

		<!-- Navigation: icons remain at exact same (x, y) coordinates in both states -->
		<nav class="flex-1 px-3 py-3 space-y-1 overflow-y-auto overflow-x-hidden">
			{#each filteredItems as item}
				{@const active = isActive(item.path)}
				<a
					href={item.path}
					aria-label={item.label}
					title={collapsed ? item.label : undefined}
					class="h-10 w-full flex items-center pl-[20px] pr-3 rounded-lg transition-colors duration-150 group
					{active
						? 'text-[var(--primary)] bg-[var(--primary)]/10 font-semibold'
						: 'text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/70'}"
				>
					<span
						aria-hidden="true"
						class="material-symbols-outlined shrink-0 transition-transform duration-150 {active ? 'fill-1' : ''}"
						style="font-size: 20px"
					>
						{item.icon}
					</span>
					<span
						class="text-sm font-medium whitespace-nowrap overflow-hidden transition-all duration-200 ease-out {collapsed
							? 'max-w-0 opacity-0 ml-0'
							: 'max-w-[160px] opacity-100 ml-3'}"
					>
						{item.label}
					</span>
				</a>
			{/each}
		</nav>

		<!-- Footer: fixed row heights so nothing shifts vertically when toggling -->
		<div class="p-2.5 border-t border-[var(--ghost-border)] space-y-1.5 shrink-0">
			<div
				class="h-12 flex items-center pl-[16px] pr-3 rounded-lg bg-[var(--surface-container-lowest)] border border-[var(--ghost-border)] overflow-hidden"
				title={collapsed ? `${$auth.user?.email || ''} (${$auth.user?.role || 'member'})` : undefined}
			>
				<div
					class="w-8 h-8 rounded-full flex items-center justify-center text-xs font-semibold text-[var(--on-brand)] shrink-0"
					style="background: var(--brand-gradient)"
				>
					{$auth.user?.email?.[0]?.toUpperCase() || '?'}
				</div>
				<div
					class="min-w-0 overflow-hidden whitespace-nowrap transition-all duration-200 ease-out {collapsed
						? 'max-w-0 opacity-0 ml-0'
						: 'max-w-[160px] opacity-100 ml-3'}"
				>
					<p class="text-sm font-medium text-[var(--on-surface)] truncate">{$auth.user?.email}</p>
					<p class="text-[10px] uppercase tracking-wider text-[var(--on-surface-variant)]">
						{$auth.user?.role || 'member'}
					</p>
				</div>
			</div>

			<div class="h-9 flex items-center gap-1">
				<button
					type="button"
					aria-label="Toggle theme"
					title={collapsed ? `Switch to ${$theme === 'light' ? 'Dark' : 'Light'} mode` : undefined}
					class="flex-1 h-9 flex items-center justify-center rounded-lg text-xs font-medium text-[var(--on-surface-variant)] hover:text-[var(--on-surface)] hover:bg-[var(--surface-container)]/50 transition-colors cursor-pointer overflow-hidden"
					onclick={() => theme.toggle()}
				>
					<span aria-hidden="true" class="material-symbols-outlined shrink-0" style="font-size: 16px">
						{$theme === 'light' ? 'dark_mode' : 'light_mode'}
					</span>
					<span
						class="whitespace-nowrap overflow-hidden transition-all duration-200 ease-out {collapsed
							? 'max-w-0 opacity-0 ml-0'
							: 'max-w-[60px] opacity-100 ml-1.5'}"
					>
						{$theme === 'light' ? 'Dark' : 'Light'}
					</span>
				</button>
				<button
					type="button"
					aria-label="Logout"
					title={collapsed ? 'Logout' : undefined}
					class="flex-1 h-9 flex items-center justify-center rounded-lg text-xs font-medium text-[var(--error)] hover:bg-[var(--error)]/10 transition-colors cursor-pointer overflow-hidden"
					onclick={async () => {
						try {
							await auth.logout();
						} finally {
							window.location.href = '/login';
						}
					}}
				>
					<span aria-hidden="true" class="material-symbols-outlined shrink-0" style="font-size: 16px">logout</span>
					<span
						class="whitespace-nowrap overflow-hidden transition-all duration-200 ease-out {collapsed
							? 'max-w-0 opacity-0 ml-0'
							: 'max-w-[60px] opacity-100 ml-1.5'}"
					>
						Logout
					</span>
				</button>
			</div>
		</div>
	</aside>

	<main class="flex-1 min-w-0 overflow-auto">
		<div class="p-4 sm:p-6 lg:p-8 max-w-[1400px] mx-auto">
			{@render children()}
		</div>
	</main>
</div>
